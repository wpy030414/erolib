# Migration: MSI-installed EroLib → dev build with the login fix

If you've been running the MSI-installed EroLib (the version under
`Q:\TESTSFWR\Dev\MSI_Installed\Erolib\`) and Pixiv/EHentai login fails, your
build was produced **before** the WebView2 `--disable-features=AppBoundEncryption`
flag was added. The MSI ships the cookie-data-dir separation (so login windows
no longer crash with `0x8007139F ERROR_INVALID_STATE`) but the cookie SQLite
it writes is still App-Bound-encrypted — meaning the new plaintext-only
capture path returns nothing and the UI stays on "please log in".

Three steps to get a working login. None of them touch your library data.

## Step 1 — Nuke the encrypted cookie stores

The `v10…` blobs WebView2 wrote under your old profile key are not reusable
on the new plaintext profile. Wipe the three EBWebView folders only —
keep `library/`, `covers/`, `cache/`, and `manga-manager.db`.

```powershell
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\im.xrl.erolib\EBWebView"
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\im.xrl.erolib\EBWebView-login-pixiv"
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\im.xrl.erolib\EBWebView-login-ehentai"
```

## Step 2 — Build + install the fresh MSI from the dev source

```powershell
cd "Q:\TESTSFWR\Dev\3rolib"
pnpm install
pnpm tauri:build
# MSI lands at: src-tauri\target\release\bundle\msi\*.msi
```

Uninstall the previous EroLib first (`Apps & Features → EroLib → Uninstall`)
to release any mutex the old WebView2 process group is holding, then
double-click the freshly built MSI (or
`msiexec /i "src-tauri\target\release\bundle\msi\erolib_26.7.12+0240_x64_en-US.msi"`).

## Step 3 — Re-login once per service

1. In a normal browser, log **out** of Pixiv and EHentai. Otherwise the
   `v10` blob from your last browser session will be replayed onto the new
   plaintext store.
2. Open EroLib → Pixiv → "Login". Complete the OAuth in the in-app window.
   The window may sit on `accounts.pixiv.net` for a few seconds after
   submission — that is expected (the OAuth return_to chain); the polling
   loop now accepts both `www.pixiv.net` and `accounts.pixiv.net` hosts.
3. Repeat for EHentai.
4. Verify the new SQLite has plaintext cookies:

   ```powershell
   & "Q:\msys64\ucrt64\bin\sqlite3.exe" `
     "$env:LOCALAPPDATA\im.xrl.erolib\EBWebView-login-pixiv\EBWebView\Default\Network\Cookies" `
     "SELECT name, length(value), length(encrypted_value) FROM cookies WHERE host_key LIKE '%pixiv%';"
   ```

   You should see `length(value) > 0` and `length(encrypted_value) == 0`
   on the `PHPSESSID` row. If `length(encrypted_value) > 0`, the new build's
   `--disable-features=AppBoundEncryption` flag did not take — confirm
   with `cargo test --manifest-path src-tauri/Cargo.toml --bin erolib bdd`
   and look at the running process command line.

## Why the MSI build is missing this

The MSI ships without the `additional_browser_args("--disable-features=AppBoundEncryption")`
call in `src-tauri/src/commands/pixiv_login.rs`. That line is **uncommitted**
on the working tree as of this writing — the commit `ad6c628` was tagged
before the WebView2 investigation. A rebundle from the current working tree
produces an MSI with the flag in place.

## What is preserved across migration

- `library/` — your local books folder.
- `covers/` — book covers.
- `cache/` — thumbnails and other cached media.
- `manga-manager.db` (+ `-shm`, `-wal`) — your library SQLite.
- `pixiv_session.json`, `ehentai_session.json` — keep these if you want
  your prior `PHPSESSID` (only useful if it's still valid; if the session
  has expired on the server, just re-login once per service).