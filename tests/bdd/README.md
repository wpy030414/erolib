# EroLib BDD — Living Documentation

This directory contains the **behaviour specifications** that drive the Pixiv
and eHentai login flows. Each `.feature` file is human-readable, and every
scenario maps 1:1 to a Rust unit test under `src-tauri/src/`.

## Why no step definitions / Cucumber glue?

We deliberately avoided `@cucumber/gherkin` + a JS step runner. The login
behaviours are pure Rust, and bridging Gherkin→JS→Rust adds two layers of
duplication for zero coverage gain. Instead:

- `.feature` files are the **spec** (what we promise).
- `#[cfg(test)] mod bdd` blocks in Rust are the **executable spec** (the
  behaviour in code). Each Rust test names a scenario in its doc comment so a
  failing test points to the right scenario.
- `cargo test --manifest-path src-tauri/Cargo.toml --bin erolib bdd` is the
  runner.

## Coverage matrix

| Scenario                                                       | Test                                                                          | File                                      |
|----------------------------------------------------------------|-------------------------------------------------------------------------------|-------------------------------------------|
| PHPSESSID parses to user id                                    | `bdd_pixiv::phpsessid_extracts_user_id`                                       | `src-tauri/src/services/pixiv.rs`         |
| Cookie without PHPSESSID → no user id                          | `bdd_pixiv::cookie_without_phpsessid_returns_none`                            | `src-tauri/src/services/pixiv.rs`         |
| PHPSESSID non-numeric prefix → no user id                      | `bdd_pixiv::phpsessid_non_numeric_returns_none`                               | `src-tauri/src/services/pixiv.rs`         |
| PHPSESSID empty prefix → no user id                            | `bdd_pixiv::phpsessid_empty_prefix_returns_none`                              | `src-tauri/src/services/pixiv.rs`         |
| Fallback: 302 `/users/<id>/setting` → user id                  | `bdd_pixiv::fallback_extracts_user_id_from_302`                               | `src-tauri/src/services/pixiv.rs`         |
| Fallback: 200 with no Location → Err                           | `bdd_pixiv::fallback_returns_err_on_200_no_location`                          | `src-tauri/src/services/pixiv.rs`         |
| Fallback: 302 with empty Location → Err                        | `bdd_pixiv::fallback_returns_err_on_empty_location`                           | `src-tauri/src/services/pixiv.rs`         |
| `user_id_from_setting_location` parses path with query string  | `bdd_pixiv::setting_location_parses_path_with_query`                          | `src-tauri/src/services/pixiv.rs`         |
| `user_id_from_setting_location` rejects non-`/users/` paths    | `bdd_pixiv::setting_location_rejects_non_users`                               | `src-tauri/src/services/pixiv.rs`         |
| WebView2 SQLite plaintext PHPSESSID captured                   | `bdd::capture_plaintext_phpsessid_succeeds`                                   | `src-tauri/src/commands/cookies.rs`       |
| Non-matching hosts excluded                                    | `bdd::capture_excludes_non_matching_hosts`                                    | `src-tauri/src/commands/cookies.rs`       |
| Encrypted-only rows ignored                                    | `bdd::capture_returns_none_when_value_empty_encrypted`                        | `src-tauri/src/commands/cookies.rs`       |
| EHentai ipb cookies captured                                   | `bdd::capture_ehentai_returns_ipb_cookies`                                   | `src-tauri/src/commands/cookies.rs`       |
| `has_pixiv_session` predicate                                  | `bdd::has_pixiv_session_accepts_phpsessid` / `…_rejects_when_missing`         | `src-tauri/src/commands/cookies.rs`       |
| `has_ehentai_session` predicate                                | `bdd::has_ehentai_session_requires_both_cookies`                              | `src-tauri/src/commands/cookies.rs`       |
| Captured login persists to `pixiv_session.json`                | `bdd_pixiv_session::captured_login_writes_session_file`                       | `src-tauri/src/commands/pixiv.rs`         |
| Corrupted `pixiv_session.json` ignored at startup              | `bdd_pixiv_session::corrupted_session_file_is_ignored_at_startup`             | `src-tauri/src/commands/pixiv.rs`         |

## How to run

```powershell
cd Q:\TESTSFWR\Dev\3rolib
cargo test --manifest-path src-tauri/Cargo.toml --bin erolib bdd
```

Expected last line: `test result: ok. 18 passed; 0 failed`.

## What is NOT covered here

- **GUI flow** (clicking the login button, OAuth completion) — those require
  WebView2 + user interaction. The HTTP-level capture behaviour is what
  fails in production, and that is what these tests cover.
- **Live network path** in `PixivClient::fetch_current_user_id` — the fallback
  redirect-scrape logic is exercised via a `StubFetcher` (no real reqwest
  client, no network), so a live `setting_user.php` round-trip is still
  uncovered. The cookie-string parse and the redirect-to-id parsing logic are
  both covered.

## How to add a new scenario

1. Add the scenario to the appropriate `.feature` (ubiquitous language:
   "Pixiv login", "eHentai login", "cookie capture", "session persistence").
2. Add the corresponding `#[test]` to the matching `bdd` module.
3. Reference the scenario in the test's doc comment so a failing test points
   to the spec line that defined the behaviour.
4. Re-run `cargo test … bdd` — all green.