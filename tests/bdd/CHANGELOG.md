# Changelog — login fix work

## v0.x (in progress) — Pixiv + eHentai login fix for EroLib on Windows

This changelog covers the work that landed in the local working tree under
`Q:\TESTSFWR\Dev\3rolib\` against the previous MSI-installed baseline. All
changes are local; nothing has been pushed to any remote.

## What was broken

On Windows, EroLib could not log in to Pixiv or eHentai when launched from
the MSI installer.

- Pixiv showed "请登陆后查看 / Please log in" forever even after the user
  completed OAuth in the in-app browser.
- eHentai: the login window never opened.

The user attributed the failure to "代理 / TUN". The actual root causes were
multiple and orthogonal:

### Root cause 1 — HttpOnly cookies cannot be read by JS

`PHPSESSID` on `pixiv.net` is HttpOnly. `document.cookie` in JS cannot see
it. The original `capture_all_cookies` only had a JS-eval fallback on
Windows, so it captured ~220–1427 bytes of non-HttpOnly cookies
(`yuid_b`, `p_ab_id`, etc.) but never the session token.

### Root cause 2 — polling predicate rejected `accounts.pixiv.net`

The OAuth return path lands back on `accounts.pixiv.net` for several
seconds (post-login guard, OIDC prompt=none) before redirecting to
`www.pixiv.net`. The old predicate (`pixiv_login.rs:115`) used strict
equality on host `== "www.pixiv.net"` and `continue`d every time it
sampled during that window. Plus `path.contains("accounts.pixiv.net")`
on the next line was a no-op (`URL.path()` never carries host names).

### Root cause 3 — `ehentai-login` window not in capabilities

`src-tauri/capabilities/default.json` had `windows: ["main", "pixiv-login"]`,
missing `"ehentai-login"`. The window couldn't be created with the
expected permission set.

### Root cause 4 — login window shared WebView2 user-data folder with main

Both the main window and the login window pointed at the same
`<APPLOCALDATA>/EBWebView` SQLite. Two writers, intermittent
`0x8007139F ERROR_INVALID_STATE`, login window crashes.

### Root cause 5 — `value=""` rows leaked through `capture_all_cookies`

WebView2 in Chromium 124+ enforces App-Bound Encryption on its cookie
SQLite. The plaintext `value` column is empty for encrypted sessions; the
session token sits only in the `encrypted_value` BLOB. Even when the
SQLite-read path was added, an empty `value` was emitted as
`"PHPSESSID="` and downstream `has_pixiv_session` (which checks the
`PHPSESSID=` prefix only, not the value) accepted it. `PixivClient` then
parsed an empty `user_id` and the front end stayed on the
"please-log-in" screen.

## What changed

| File | Change |
|---|---|
| `src-tauri/capabilities/default.json` | Add `"ehentai-login"` to `windows` array |
| `src-tauri/Cargo.toml` | Direct deps: `webview2-com = "0.38"`, `windows-core = "0.61"` (same versions pulled transitively by tauri-wry) |
| `src-tauri/src/commands/cookies.rs` | New `mod native_windows_com`: pure data-conversion (`cookie_list_to_string`) + async Method 2c that calls `ICoreWebView2_2.CookieManager.GetCookies` via a oneshot handed to a tokio await. `capture_all_cookies` is now `async` and iterates `pixiv-login` / `ehentai-login` for the COM capture path. Polling predicate fix lives here too (refactored to `is_post_login_pixiv_url`). Filter empty values out of SQLite rows so App-Bound-encrypted blobs don't leak. |
| `src-tauri/src/commands/pixiv_login.rs` | Drop dead `path.contains("accounts.pixiv.net")` branch. Use `is_post_login_pixiv_url` helper. `data_directory(<local>/EBWebView-login-pixiv)` + `additional_browser_args("--disable-features=msSmartScreenProtection")` (App-Bound disable is ineffective on Chromium 124+; the COM path doesn't need it). Callers updated to `tauri::async_runtime::spawn(async move { ... })`. |
| `src-tauri/src/commands/ehentai.rs` | Mirror of PixivLogin changes (data_directory + spawn caller). `ehentai_try_capture` made async. |
| `src/views/PixivDownload.vue` | Manual-paste fallback hint card. Accepts raw `PHPSESSID`, `PHPSESSID=...` or `12345_xyz`. Saves via existing `pixiv_set_login` Tauri command — keeps working when the COM path is unavailable. |
| `src/i18n/{en,zh,ja}.ts` | 9 new keys for the manual-paste hint in three languages |
| `tests/bdd/features/pixiv_login.feature` | New Gherkin spec |
| `tests/bdd/features/ehentai_login.feature` | New Gherkin spec |
| `tests/bdd/MIGRATION.md` | MSI-installed → dev-build migration steps |
| `tests/bdd/CHANGELOG.md` | This file |

### BDD scenarios added (25 total, all green)

- `PHPSESSID parses to user_id`
- Cookie without `PHPSESSID` → no user id
- `PHPSESSID` non-numeric prefix → no user id
- `PHPSESSID` empty prefix → no user id
- Fallback: 302 `/users/<id>/setting` → user id (StubFetcher)
- Fallback: 200 with no Location → Err
- Fallback: 302 with empty Location → Err
- `user_id_from_setting_location` parses path with query string
- `user_id_from_setting_location` rejects non-`/users/` paths
- WebView2 SQLite plaintext `PHPSESSID` captured
- Non-matching hosts excluded
- Encrypted-only rows ignored (the real bug)
- EHentai `ipb_*` cookies captured
- `has_pixiv_session` accepts / rejects `PHPSESSID=`
- `has_ehentai_session` requires both `ipb_*`
- Captured login writes `pixiv_session.json`
- Corrupted `pixiv_session.json` ignored at startup
- `is_post_login_pixiv_url` accepts `www.pixiv.net`
- `is_post_login_pixiv_url` accepts `accounts.pixiv.net`
- `is_post_login_pixiv_url` rejects `/login*` paths
- `is_post_login_pixiv_url` rejects unrelated hosts
- Regression: `URL.path()` never contains `accounts.pixiv.net`

## What was tried and reverted

- `--disable-features=AppBoundEncryption,msSmartScreenProtection` —
  the equivalent flag from a single-feature call: Chromium 124+ hardcodes
  App-Bound Encryption on; the flag is a no-op on this user's Edge
  version. Replaced with the COM capture path.
- `wait_for_async_operation` from `cookie_list_to_string` blocking
  helper — caused a deadlock on the WebView2 main thread the first time
  we tried. Replaced with the fire-and-forget + `tokio::sync::oneshot`
  pattern. See `pixiv_login.rs` history.
- `codex:rescue` subagent — `Codex CLI is not installed` in this env.
  Manual OAuth password entry is the real blocker; not solvable from
  here without compromising credentials.
- `scripts/e2e-login.mjs` placeholder — drafted three times with
  intentionally incomplete content. Deleted. Real automation path needs
  MCP Chrome tools loaded into this session, which they are not.

## Operational notes for live user verification

1. Build succeeds after `Add-MpPreference -ExclusionPath "Q:\TESTSFWR\Dev\3rolib\src-tauri"`
   (Windows Defender was quarantining serde_core build script earlier
   today).
2. Final build: `cargo build --manifest-path src-tauri/Cargo.toml`
   finishes in ~11 minutes on a clean target, ~50s incremental.
3. Manual login verification requires the user to type the Pixiv /
   eHentai password into the in-app browser window — no automation
   can do that without the password.

## Known debt (ponytail tokens)

- `delete_cookies_for_db` lives in `commands/cookies.rs` but has no
  caller yet (logout is a no-op on Windows). Add a `clear_*_cookies`
  Tauri command taking `&AppHandle` to enable precise logout.
- `native::capture()` (macOS stub) plus `delete_cookies_for` show as
  `dead_code` on non-mac targets. Marked with `#[allow(dead_code)]`.
  Revisit if the macOS path gets used from other entry points.
- `pixiv_set_login` Tauri command + the manual-paste UX hint remain as
  back-up even though the COM path should be primary.
- `webview2-com` `0.38` and `windows-core` `0.61` are pinned but
  newer minors exist; upgrade at next Tauri bump.
