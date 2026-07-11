use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;

const DEFAULT_ENDPOINT: &str = "http://localhost:6800/jsonrpc";

/// Per-poll progress snapshot for one aria2 gid, handed to the
/// `wait_for_gid_with_progress` callback so callers can aggregate byte progress
/// across concurrent downloads (not just instantaneous speed).
pub struct ProgressUpdate {
    pub speed: u64,
    pub completed_length: u64,
    pub total_length: u64,
}

/// Lazy-initialised JSON-RPC client for aria2. Construction is cheap and
/// non-blocking; the first download call triggers a connection attempt and, if
/// needed, a local aria2c daemon spawn.
pub struct Aria2Client {
    /// Used to resolve the bundled aria2c binary (resource dir / exe dir).
    app: AppHandle,
    endpoint: String,
    token: Option<String>,
    http: Client,
    /// Inner state initialised on first use.
    inner: Mutex<Option<Aria2Inner>>,
}

struct Aria2Inner {
    /// Held so the child process stays alive for the lifetime of the client.
    #[allow(dead_code)]
    daemon: Option<Child>,
}

impl Aria2Client {
    /// Build a new lazy client.  No I/O is performed until the first download.
    pub fn new(app: AppHandle) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("build aria2 http client")?;
        Ok(Self {
            app,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            token: None,
            http,
            inner: Mutex::new(None),
        })
    }

    /// Resolve the aria2c executable to launch.
    ///
    /// The bundled binary lives under `binaries/aria2c-bin/<os>/` and is
    /// resolved per compile target:
    ///   - macOS   → `macos/aria2c` — self-contained via dylibbundler, its
    ///               dylibs rewritten to `@executable_path/libs/`.
    ///   - Windows → `windows/aria2c.exe` — the official static PE32+ x86-64
    ///               build (no DLLs needed).
    ///
    /// Priority:
    /// 1. The bundled binary shipped as a Tauri resource
    ///    (`.app/Contents/Resources/...` / `<exe-dir>/resources/...`).
    /// 2. The same path relative to the running executable / `Contents/Resources`
    ///    (defensive fallback if `resource_dir()` resolves elsewhere).
    /// 3. `"aria2c"`/`"aria2c.exe"` on `PATH` (e.g. `brew install aria2` during
    ///    `tauri dev`, or any platform without a bundled binary).
    fn resolve_aria2c_binary(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        let rel = Path::new("binaries")
            .join("aria2c-bin")
            .join("macos")
            .join("aria2c");
        #[cfg(target_os = "windows")]
        let rel = Path::new("binaries")
            .join("aria2c-bin")
            .join("windows")
            .join("aria2c.exe");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            // No bundled binary on other platforms — defer to a PATH lookup.
            return PathBuf::from("aria2c");
        }

        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(resource_dir) = self.app.path().resource_dir() {
            candidates.push(resource_dir.join(&rel));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                candidates.push(exe_dir.join(&rel));
                if let Some(contents) = exe_dir.parent() {
                    candidates.push(contents.join("Resources").join(&rel));
                }
            }
        }
        for candidate in candidates {
            if candidate.is_file() {
                return candidate;
            }
        }
        #[cfg(target_os = "windows")]
        {
            PathBuf::from("aria2c.exe")
        }
        #[cfg(not(target_os = "windows"))]
        {
            PathBuf::from("aria2c")
        }
    }

    /// Initialise the inner state on first call.  Safe to call multiple times.
    async fn ensure_initialised(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        // Try to reach a running daemon first.
        if self.is_reachable().await {
            *guard = Some(Aria2Inner { daemon: None });
            return Ok(());
        }
        // Spawn a local daemon.
        let daemon = self.try_spawn_daemon().await?;
        *guard = Some(Aria2Inner { daemon: Some(daemon) });
        Ok(())
    }

    async fn is_reachable(&self) -> bool {
        match self.call("aria2.getVersion", vec![]).await {
            Ok(v) => {
                tracing::info!(target: "erolib::aria2", "aria2 daemon reachable: {v:?}");
                true
            }
            Err(e) => {
                tracing::debug!(target: "erolib::aria2", %e, "aria2 daemon not reachable");
                false
            }
        }
    }

    async fn try_spawn_daemon(&self) -> Result<Child> {
        tracing::info!(target: "erolib::aria2", "attempting to spawn local aria2c daemon");

        let data_dir = dirs::data_local_dir()
            .map(|d| d.join("erolib").join("aria2"))
            .unwrap_or_else(|| PathBuf::from("."));
        let _ = std::fs::create_dir_all(&data_dir);
        let session_file = data_dir.join("aria2.session");
        let log_file = data_dir.join("aria2.log");

        let mut cmd = {
            let bin = self.resolve_aria2c_binary();
            tracing::info!(target: "erolib::aria2", "aria2c binary: {}", bin.display());
            // The bundled binary is a Tauri resource; resource files may not
            // retain their executable bit after being copied into the bundle,
            // so (re-)apply it before spawning. Only for a concrete file path,
            // never for the `PATH` fallback.
            #[cfg(unix)]
            if bin.is_file() {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&bin) {
                    let mut perms = meta.permissions();
                    if perms.mode() & 0o111 == 0 {
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&bin, perms);
                    }
                }
            }
            Command::new(bin)
        };
        cmd.arg("--enable-rpc")
            .arg("--rpc-listen-port=6800")
            .arg("--rpc-allow-origin-all=true")
            .arg("--rpc-listen-all=false")
            .arg("--continue=true")
            .arg("--split=2")
            .arg("--max-connection-per-server=2")
            // Raised above the per-task Rust semaphore (8): two concurrent
            // tasks each submit up to 8 add_uri, so 16 lets both run wide
            // without aria2 queueing gids that wait_for_gid (no timeout) polls
            // indefinitely.
            .arg("--max-concurrent-downloads=16")
            .arg("--max-tries=5")
            .arg("--retry-wait=5")
            .arg("--timeout=60")
            .arg(format!("--save-session={}", session_file.display()))
            .arg(format!("--log={}", log_file.display()))
            .arg("--log-level=warn")
            .arg("--daemon=false")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().context(
            "failed to spawn the bundled aria2c binary; the app may be damaged, \
             or start an aria2 daemon on localhost:6800 manually",
        )?;

        // Drain stderr so the pipe doesn't block.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "erolib::aria2", "{line}");
                }
            });
        }

        // Wait up to 5 seconds for the daemon to accept RPC.
        let deadline = sleep(Duration::from_secs(5));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => {
                    let _ = child.start_kill();
                    anyhow::bail!("aria2c daemon did not become ready within 5 seconds");
                }
                _ = sleep(Duration::from_millis(200)) => {
                    if self.is_reachable().await {
                        tracing::info!(target: "erolib::aria2", "local aria2c daemon ready");
                        return Ok(child);
                    }
                }
            }
        }
    }

    async fn call(
        &self,
        method: &str,
        mut params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        if let Some(token) = &self.token {
            let secret = format!("token:{token}");
            params.insert(0, json!(secret));
        }

        let body = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            method: method.to_string(),
            params,
        };

        let resp = self
            .http
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .context("aria2 rpc request failed")?;

        let status = resp.status();
        let text = resp.text().await.context("read aria2 rpc response")?;
        if !status.is_success() {
            anyhow::bail!("aria2 rpc returned HTTP {status}: {text}");
        }

        let parsed: RpcResponse = serde_json::from_str(&text)
            .with_context(|| format!("parse aria2 rpc response: {text}"))?;

        if let Some(err) = parsed.error {
            anyhow::bail!("aria2 rpc error {}: {}", err.code, err.message);
        }

        parsed.result.context("aria2 rpc response missing result")
    }

    /// Add a single URI and return the aria2 gid.
    pub async fn add_uri(
        &self,
        uri: &str,
        referer: Option<&str>,
        out: Option<&str>,
        dir: Option<&Path>,
    ) -> Result<String> {
        self.ensure_initialised().await?;

        let mut options = serde_json::Map::new();
        options.insert("max-tries".to_string(), json!("5"));
        options.insert("retry-wait".to_string(), json!("5"));
        options.insert("continue".to_string(), json!("true"));
        options.insert("split".to_string(), json!("2"));
        options.insert("max-connection-per-server".to_string(), json!("2"));
        options.insert(
            "user-agent".to_string(),
            json!("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36"),
        );

        if let Some(r) = referer {
            options.insert("referer".to_string(), json!(r));
        }
        if let Some(o) = out {
            options.insert("out".to_string(), json!(o));
        }
        if let Some(d) = dir {
            options.insert("dir".to_string(), json!(d.to_string_lossy().to_string()));
        }

        let result = self
            .call("aria2.addUri", vec![json!([uri]), json!(options)])
            .await?;
        result
            .as_str()
            .map(|s| s.to_string())
            .context("aria2.addUri returned non-string gid")
    }

    /// Poll aria2 while awaiting `on_progress(ProgressUpdate)` on every poll so
    /// the caller can persist live byte progress + speed. Returns the completed
    /// file path. The callback is async because persisting needs `.await` — a
    /// sync callback would silently drop the future. `paused` is checked each
    /// poll: while set, the download keeps spinning in place (aria2 keeps its
    /// in-progress state) until resumed or cancelled.
    pub async fn wait_for_gid_with_progress<F, Fut>(
        &self,
        gid: &str,
        poll_interval: Duration,
        cancelled: &AtomicBool,
        paused: &AtomicBool,
        mut on_progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(ProgressUpdate) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                let _ = self.remove(gid).await;
                anyhow::bail!("cancelled");
            }
            while paused.load(Ordering::Relaxed) {
                if cancelled.load(Ordering::Relaxed) {
                    let _ = self.remove(gid).await;
                    anyhow::bail!("cancelled");
                }
                sleep(poll_interval).await;
            }

            let status = self.tell_status(gid).await?;
            let state = status
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Snapshot this gid's byte progress every poll so the caller can
            // aggregate across concurrent downloads. completed/total lengths are
            // absent until aria2 receives the Content-Length, hence unwrap_or 0.
            let upd = ProgressUpdate {
                speed: status
                    .get("downloadSpeed")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0),
                completed_length: status
                    .get("completedLength")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0),
                total_length: status
                    .get("totalLength")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0),
            };
            on_progress(upd).await;

            match state {
                "complete" => {
                    let files = status
                        .get("files")
                        .and_then(|v| v.as_array())
                        .context("missing files in aria2 status")?;
                    let first = files.first().context("no files in aria2 status")?;
                    let path = first
                        .get("path")
                        .and_then(|v| v.as_str())
                        .context("missing path in aria2 status")?;
                    return Ok(PathBuf::from(path));
                }
                "error" => {
                    let err_msg = status
                        .get("errorMessage")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown aria2 error");
                    anyhow::bail!("aria2 download failed: {err_msg}");
                }
                "removed" => {
                    anyhow::bail!("aria2 download removed");
                }
                _ => {
                    sleep(poll_interval).await;
                }
            }
        }
    }

    pub async fn tell_status(&self, gid: &str) -> Result<serde_json::Value> {
        self.call("aria2.tellStatus", vec![json!(gid)]).await
    }

    pub async fn remove(&self, gid: &str) -> Result<()> {
        let _ = self.call("aria2.remove", vec![json!(gid)]).await;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: String,
    method: String,
    params: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Debug, Clone, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}
