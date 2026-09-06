Feature: EHentai login & cookie capture
  To browse and download from e-hentai / exhentai
  As an EroLib user
  I want to sign in via the in-app browser and have the session remembered

  Rule: ipb_member_id and ipb_pass_hash together constitute a valid session
    Example: Both required cookies present
      Given the captured cookie is "ipb_member_id=42; ipb_pass_hash=deadbeef; igneous=x"
      When the client checks the EHentai session
      Then the session is valid

    Example: Only ipb_member_id is incomplete
      Given the captured cookie is "ipb_member_id=42; igneous=x"
      When the client checks the EHentai session
      Then the session is invalid

    Example: Only ipb_pass_hash is incomplete
      Given the captured cookie is "ipb_pass_hash=deadbeef; igneous=x"
      When the client checks the EHentai session
      Then the session is invalid

  Rule: The WebView2 cookie SQLite feeds the eH capture path
    Example: A cookie SQLite with both ipb cookies is captured
      Given a WebView2 cookies SQLite with rows
        | host_key        | name           | value     | is_httponly |
        | .e-hentai.org   | ipb_member_id  | 42        | 0           |
        | .e-hentai.org   | ipb_pass_hash  | deadbeef  | 0           |
        | .exhentai.org   | igneous        | x         | 0           |
      When capture_all_cookies reads the SQLite
      Then it returns a string containing "ipb_member_id=42"
      And it returns a string containing "ipb_pass_hash=deadbeef"
      And the result is reported as captured with has_session true