Feature: Pixiv login & cookie capture
  To download or browse a logged-in Pixiv account
  As an EroLib user
  I want to sign in to Pixiv inside the app and have the session remembered

  Background:
    Given the WebView2 user-data folder for the Pixiv login window exists at
      | subdir                    |
      | EBWebView-login-pixiv     |

  Rule: PHPSESSID is required for any authenticated request
    Example: A captured cookie string with PHPSESSID parses to a user id
      Given the captured cookie is "PHPSESSID=12345_abc; yuid_b=foo; p_ab_id=bar"
      When the client extracts the user id from the cookie
      Then the returned user id is "12345"

    Example: A cookie without PHPSESSID cannot authenticate
      Given the captured cookie is "yuid_b=foo; p_ab_id=bar; category_mask=1"
      When the client extracts the user id from the cookie
      Then no user id is returned

    Example: PHPSESSID with non-numeric user id is rejected
      Given the captured cookie is "PHPSESSID=notdigits_secret; yuid_b=foo"
      When the client extracts the user id from the cookie
      Then no user id is returned

    Example: PHPSESSID where the prefix is empty is rejected
      Given the captured cookie is "PHPSESSID=_onlysecret; yuid_b=foo"
      When the client extracts the user id from the cookie
      Then no user id is returned

  Rule: HttpOnly PHPSESSID lives in the WebView2 cookie SQLite and is unreachable to JS-eval
    Example: A WebView2 SQLite row with plaintext PHPSESSID is captured
      Given a WebView2 cookies SQLite with rows
        | host_key       | name      | value     | is_httponly |
        | .pixiv.net     | PHPSESSID | 99999_xyz | 1           |
        | .pixiv.net     | yuid_b    | y-b       | 0           |
        | www.pixiv.net  | a_type    | 1         | 0           |
      When capture_all_cookies reads the SQLite
      Then it returns a string containing "PHPSESSID=99999_xyz"
      And it returns a string containing "yuid_b=y-b"
      And the result is reported as captured with has_session true

    Example: A WebView2 SQLite row with all-encrypted values cannot be read
      Given a WebView2 cookies SQLite with rows
        | host_key   | name      | value | is_httponly | encrypted_value_len |
        | .pixiv.net | PHPSESSID |       | 1           | 105                 |
      When capture_all_cookies reads the SQLite
      Then the returned string does not contain "PHPSESSID="
      And it falls through to the JS-eval fallback

    Example: Cookies from non-matching hosts are excluded
      Given a WebView2 cookies SQLite with rows
        | host_key      | name      | value      |
        | .example.com  | PHPSESSID | 99999_xyz  |
        | .pixiv.net    | PHPSESSID | 11111_abc  |
      When capture_all_cookies reads the SQLite
      Then the returned string contains "PHPSESSID=11111_abc"
      And the returned string does not contain "99999_xyz"

  Rule: The session persists across app restarts
    Example: A captured login is written to pixiv_session.json
      Given a Pixiv login with user_id "77777" and cookie "PHPSESSID=77777_aaa"
      When the login window finishes the capture flow
      Then pixiv_session.json contains the cookie "PHPSESSID=77777_aaa"
      And pixiv_session.json contains user_id "77777"

    Example: A corrupted pixiv_session.json is ignored at startup
      Given pixiv_session.json contains invalid JSON
      When the app starts and restores Pixiv login
      Then the saved login is treated as absent
      And no error is raised to the user