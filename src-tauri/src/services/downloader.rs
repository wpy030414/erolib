//! Native streaming file downloader — replaces the bundled aria2 daemon.
//!
//! One `Downloader` backs every task-worker page download, the ugoira zip
//! fetch, and app-update downloads. Semantics carried over from the aria2
//! pipeline it replaces, plus one fix:
//!
//! - headers: Chrome UA + optional `Referer` / `Origin` (image CDNs gate on
//!   these — Pixiv's `i.pximg.net` and NiceCat's CDN both require them);
//! - retries: 5 attempts with 1s→2s→5s→5s backoff (aria2's inner
//!   `--max-tries=5 --retry-wait=5` and the old outer 3-attempt loop,
//!   folded into one schedule), cancel-aware between attempts;
//! - stall guard: 30s per-chunk activity timeout (was aria2 `--timeout=60`)
//!   so a half-open TLS connection fails loudly instead of hanging forever;
//! - **atomic publish**: bytes stream into `{out}.part` and are renamed to
//!   `{out}` only once complete. The old aria2 setup wrote straight to the
//!   final name, so a killed download could leave a truncated file that the
//!   resume cache then mistook for a full page (≥200 bytes heuristic);
//! - pause: checked before each attempt and during backoff — matching the
//!   old behaviour where an in-flight transfer simply kept flowing and only
//!   new work was held back;
//! - progress: throttled to ~250ms, reports completed/total bytes and a
//!   windowed speed; `total` is the Content-Length when the server sends
//!   one (0 otherwise).
//!
//! The client routes through [`crate::services::proxy`] detection, so a
//! Clash/V2Ray system proxy is picked up with zero configuration.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

/// Same UA the aria2 `addUri` options used to send.
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Backoff between failed attempts (attempt N sleeps BACKOFFS[N-1]); the
/// attempt count itself is BACKOFFS.len() + 1 = 5.
const RETRY_BACKOFFS: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(5),
];

/// No-byte activity budget per body read. Without it a stalled connection
/// (rate-limit, half-open TLS, hung socket) blocks the worker forever with
/// zero network activity.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum spacing between `on_progress` calls.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Throttled progress snapshot for one in-flight download.
pub struct ProgressReport {
    /// Bytes received so far.
    pub completed: u64,
    /// Content-Length when known, 0 otherwise.
    pub total: u64,
    /// Windowed throughput in bytes/sec over the last report interval.
    pub speed: u64,
}

/// Shared streaming downloader (cheap to construct; one instance per
/// `TaskManager` plus ad-hoc uses like app updates).
pub struct Downloader {
    http: reqwest::Client,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        let http = crate::services::proxy::install(reqwest::Client::builder())
            .user_agent(UA)
            .connect_timeout(Duration::from_secs(15))
            // Deliberately NO total timeout: ugoira zips and update installers
            // are tens of MB; CHUNK_TIMEOUT is the stall guard, not wall time.
            .build()
            .expect("build downloader client");
        Self { http }
    }

    /// Stream `url` to `out_dir/out_name` and return the file bytes.
    ///
    /// `min_bytes` rejects truncated/error-page bodies. `cancelled` aborts
    /// promptly (checked between chunks); `paused` holds back new attempts.
    /// `on_progress` is throttled to ~250ms, always receives one final report
    /// at completion, and is async so callers can persist progress to the DB
    /// straight from the callback (same shape as the old aria2 poll callback).
    /// On terminal failure the `.part` file is removed so the resume cache
    /// never sees a partial page.
    pub async fn download_file<F, Fut>(
        &self,
        url: &str,
        referer: Option<&str>,
        origin: Option<&str>,
        out_dir: &Path,
        out_name: &str,
        min_bytes: usize,
        cancelled: &AtomicBool,
        paused: &AtomicBool,
        mut on_progress: F,
    ) -> Result<Vec<u8>>
    where
        F: FnMut(ProgressReport) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        tokio::fs::create_dir_all(out_dir)
            .await
            .with_context(|| format!("create download dir {}", out_dir.display()))?;
        let final_path = out_dir.join(out_name);
        let part_path = out_dir.join(format!("{out_name}.part"));

        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..=RETRY_BACKOFFS.len() {
            if attempt > 0 {
                // Cancel-aware backoff so pause→cancel still aborts promptly.
                let wait = RETRY_BACKOFFS[attempt - 1];
                let mut remaining = wait;
                while remaining > Duration::ZERO {
                    if cancelled.load(Ordering::Relaxed) {
                        remove_part(&part_path).await;
                        anyhow::bail!("cancelled");
                    }
                    let tick = remaining.min(Duration::from_millis(250));
                    tokio::time::sleep(tick).await;
                    remaining -= tick;
                }
            }
            if cancelled.load(Ordering::Relaxed) {
                remove_part(&part_path).await;
                anyhow::bail!("cancelled");
            }
            while paused.load(Ordering::Relaxed) {
                if cancelled.load(Ordering::Relaxed) {
                    remove_part(&part_path).await;
                    anyhow::bail!("cancelled");
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            match self
                .attempt(url, referer, origin, &final_path, &part_path, min_bytes, cancelled, &mut on_progress)
                .await            {
                Ok(bytes) => return Ok(bytes),
                Err(e) => {
                    let e = e.context(format!("download {url}"));
                    // A cancellation surfaced from inside the stream must not
                    // burn retry attempts — re-check and bail immediately.
                    if cancelled.load(Ordering::Relaxed) {
                        remove_part(&part_path).await;
                        anyhow::bail!("cancelled");
                    }
                    last_err = Some(e);
                }
            }
        }
        remove_part(&part_path).await;
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("download failed: {url}")))
    }

    /// One download attempt: request → stream body to `.part` → atomic rename.
    async fn attempt<F, Fut>(
        &self,
        url: &str,
        referer: Option<&str>,
        origin: Option<&str>,
        final_path: &Path,
        part_path: &Path,
        min_bytes: usize,
        cancelled: &AtomicBool,
        on_progress: &mut F,
    ) -> Result<Vec<u8>>
    where
        F: FnMut(ProgressReport) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let mut req = self.http.get(url);
        if let Some(r) = referer {
            req = req.header("Referer", r);
        }
        if let Some(o) = origin {
            req = req.header("Origin", o);
        }
        let resp = req.send().await.context("send request")?;
        let status = resp.status();
        if !status.is_success() {
            // Redirects were already followed by the client (GET semantics).
            anyhow::bail!("HTTP {status}");
        }
        let total = resp.content_length().unwrap_or(0);

        let mut file = tokio::fs::File::create(part_path)
            .await
            .with_context(|| format!("create {}", part_path.display()))?;
        // Collect in memory alongside the streamed file — callers consume the
        // bytes right after (they used to `fs::read` the finished file back),
        // so this saves a re-read. Cap the preallocation in case a hostile
        // server lies with a huge Content-Length.
        let mut buf: Vec<u8> = Vec::with_capacity(total.min(8 * 1024 * 1024) as usize);

        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut completed: u64 = 0;
        let mut last_t = Instant::now();
        let mut last_bytes: u64 = 0;
        let mut last_speed: u64 = 0;
        while let Some(chunk) = tokio::time::timeout(CHUNK_TIMEOUT, stream.next())
            .await
            .map_err(|_| {
                anyhow::anyhow!("download stalled: no data for {}s", CHUNK_TIMEOUT.as_secs())
            })?
        {
            let chunk = chunk.context("read body chunk")?;
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("cancelled");
            }
            file.write_all(&chunk)
                .await
                .with_context(|| format!("write {}", part_path.display()))?;
            buf.extend_from_slice(&chunk);
            completed += chunk.len() as u64;
            if last_t.elapsed() >= PROGRESS_INTERVAL {
                let dt = last_t.elapsed().as_secs_f64();
                last_speed = ((completed - last_bytes) as f64 / dt) as u64;
                on_progress(ProgressReport {
                    completed,
                    total,
                    speed: last_speed,
                })
                .await;
                last_t = Instant::now();
                last_bytes = completed;
            }
        }
        file.flush().await.with_context(|| format!("flush {}", part_path.display()))?;
        drop(file);

        if (buf.len() as usize) < min_bytes {
            anyhow::bail!("suspiciously small file from {url} ({} bytes)", buf.len());
        }

        // Atomic publish. Windows' rename doesn't overwrite an existing
        // target, so drop a stale final file first (defensive — the resume
        // cache normally filters these before we get here).
        let _ = tokio::fs::remove_file(final_path).await;
        tokio::fs::rename(part_path, final_path)
            .await
            .with_context(|| format!("publish {}", final_path.display()))?;

        // Final report so the last partial window isn't lost.
        let dt = last_t.elapsed().as_secs_f64();
        if dt > 0.05 && completed > last_bytes {
            last_speed = ((completed - last_bytes) as f64 / dt) as u64;
        }
        on_progress(ProgressReport {
            completed,
            total,
            speed: last_speed,
        })
        .await;
        Ok(buf)
    }
}

/// Best-effort `.part` cleanup on terminal failure/cancel paths.
async fn remove_part(part_path: &Path) {
    let _ = tokio::fs::remove_file(part_path).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::Router;
    use futures_util::StreamExt as _;
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;

    /// Spawn a mock origin on a random loopback port and return its base URL.
    /// Loopback targets bypass the proxy resolver, so no proxy is involved.
    async fn spawn_server(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn idle_flags() -> (AtomicBool, AtomicBool) {
        (AtomicBool::new(false), AtomicBool::new(false))
    }

    #[tokio::test]
    async fn downloads_streams_to_disk_atomically() {
        let body: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let expected = body.clone();
        let app = spawn_server(Router::new().route(
            "/img",
            get(move || {
                let body = expected.clone();
                async move { axum::body::Body::from(body) }
            }),
        ))
        .await;
        let dir = std::env::temp_dir().join(format!("erolib-dl-test-{}", uuid::Uuid::new_v4()));
        let (cancelled, paused) = idle_flags();
        let reports: Arc<std::sync::Mutex<Vec<(u64, u64)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let dl = Downloader::new();
        let bytes = dl
            .download_file(
                &format!("{app}/img"),
                Some("https://example.org/"),
                None,
                &dir,
                "page-0001",
                200,
                &cancelled,
                &paused,
                |r| {
                    let reports = Arc::clone(&reports);
                    async move {
                        reports.lock().unwrap().push((r.completed, r.total));
                    }
                },
            )
            .await
            .unwrap();
        assert_eq!(bytes.len(), 100_000);
        assert_eq!(
            tokio::fs::read(dir.join("page-0001")).await.unwrap().len(),
            100_000
        );
        assert!(!dir.join("page-0001.part").exists(), ".part must be renamed away");
        // Progress is monotonic and the final report carries the full length.
        let reports = reports.lock().unwrap();
        assert!(reports.windows(2).all(|w| w[0].0 <= w[1].0));
        assert_eq!(reports.last().unwrap().1, 100_000);
        assert_eq!(reports.last().unwrap().0, 100_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rejects_min_bytes_and_cleans_part() {
        let app = spawn_server(Router::new().route(
            "/tiny",
            get(|| async { axum::body::Body::from(vec![0u8; 10]) }),
        ))
        .await;
        let dir = std::env::temp_dir().join(format!("erolib-dl-test-{}", uuid::Uuid::new_v4()));
        let (cancelled, paused) = idle_flags();
        let dl = Downloader::new();
        let err = dl
            .download_file(
                &format!("{app}/tiny"),
                None,
                None,
                &dir,
                "page-0001",
                200,
                &cancelled,
                &paused,
                |_| async {},
            )
            .await
            .unwrap_err();
        // anyhow's Display shows only the outer context; the root cause lives
        // in the chain (rendered by `{:#}`).
        let chain = format!("{err:#}");
        assert!(chain.contains("suspiciously small"), "{chain}");
        assert!(!dir.join("page-0001").exists(), "no final file on failure");
        assert!(!dir.join("page-0001.part").exists(), ".part cleaned on failure");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn retries_transient_failures() {
        let hits = Arc::new(AtomicU8::new(0));
        let router = {
            let hits = Arc::clone(&hits);
            Router::new().route(
                "/flaky",
                get(move || {
                    let hits = Arc::clone(&hits);
                    async move {
                        if hits.fetch_add(1, Ordering::Relaxed) < 2 {
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
                        } else {
                            axum::body::Body::from(vec![7u8; 4096]).into_response()
                        }
                    }
                }),
            )
        };
        let app = spawn_server(router).await;
        let dir = std::env::temp_dir().join(format!("erolib-dl-test-{}", uuid::Uuid::new_v4()));
        let (cancelled, paused) = idle_flags();
        let dl = Downloader::new();
        let bytes = dl
            .download_file(
                &format!("{app}/flaky"),
                None,
                None,
                &dir,
                "page-0001",
                200,
                &cancelled,
                &paused,
                |_| async {},
            )
            .await
            .unwrap();
        assert_eq!(bytes.len(), 4096);
        assert_eq!(hits.load(Ordering::Relaxed), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn cancellation_leaves_no_final_file() {
        // A handler that trickles bytes forever; we cancel from the progress
        // callback after the first report.
        let app = spawn_server(Router::new().route(
            "/trickle",
            get(|| async {
                let stream = futures_util::stream::repeat_with(|| vec![1u8; 4096])
                    .map(Ok::<_, std::convert::Infallible>);
                axum::body::Body::from_stream(stream)
            }),
        ))
        .await;
        let dir = std::env::temp_dir().join(format!("erolib-dl-test-{}", uuid::Uuid::new_v4()));
        let (cancelled, paused) = idle_flags();
        let dl = Downloader::new();
        let cancelled = &cancelled;
        let err = dl
            .download_file(
                &format!("{app}/trickle"),
                None,
                None,
                &dir,
                "page-0001",
                200,
                cancelled,
                &paused,
                |_r| async {
                    cancelled.store(true, Ordering::Relaxed);
                },
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cancelled"), "{err}");
        assert!(!dir.join("page-0001").exists(), "no final file after cancel");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
