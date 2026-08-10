# Module: 登录与 Cookie 采取

> 代码生成契约：Pixiv / EHentai 登录流程和 cookie 管理的详细行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

处理 Pixiv 和 EHentai 的应用内浏览器登录，捕获并持久化 cookie。

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/commands/pixiv_login.rs` | Pixiv 登录窗 + 轮询 |
| `src-tauri/src/commands/ehentai.rs` | EHentai 登录窗 + 轮询 |
| `src-tauri/src/commands/cookies.rs` | macOS WKHTTPCookieStore FFI |
| `src-tauri/src/commands/pixiv.rs` | Pixiv session 读写清 |
| `src/stores/pixiv-browse.ts` | 前端 Pixiv 状态 |

## 3. Tauri 命令

### Pixiv

| 命令 | 参数 | 返回 |
|---|---|---|
| `pixiv_open_login_window` | — | `void` |
| `pixiv_get_login` | — | `{ cookie, user_id, user_name? } \| null` |
| `pixiv_set_login` | `{ cookie, userId }` | `void` |
| `pixiv_clear_login` | — | `void` |

### EHentai

| 命令 | 参数 | 返回 |
|---|---|---|
| `ehentai_open_login_window` | — | `void` |
| `ehentai_get_login` | — | `string \| null`（cookie 字符串） |
| `ehentai_clear_login` | — | `void` |

## 4. Pixiv 登录流程

```
1. 开窗口 pixiv-login (520×760, 居中)
   URL: https://accounts.pixiv.net/login?return_to=https%3A%2F%2Fwww.pixiv.net%2F

2. 轮询（500ms/次，上限 1200 tick = 10 分钟）
   条件：host == "www.pixiv.net" && path 不含 "/login" && 不含 "accounts.pixiv.net"

3. 捕获 cookie（需 PHPSESSID）
   优先从 PHPSESSID 前缀解析 user_id: "{user_id}_{secret}"
   回退: setting_user.php 302 → /users/{id}/setting

4. fetch_user_name (best-effort)

5. persist → 写 pixiv_session.json

6. emit pixiv://login { user_id, cookie, user_name? }

7. 关窗
```

- 关窗销毁时兜底捕获
- 超时也兜底
- macOS 注入 spellcheck 关闭脚本（NSCorrectionPanel bug）

## 5. EHentai 登录流程

```
1. 开窗口 ehentai-login (560×760)
   URL: https://forums.e-hentai.org/index.php?act=Login

2. 轮询（500ms/次，上限 1200 tick）
   等待跳到 e-hentai.org / exhentai.org（排除 forums.e-hentai.org）
   → 避免 Cloudflare 登录表单被 JS eval 破坏

3. 捕获 cookie（需 ipb_member_id + ipb_pass_hash）

4. session.set_cookie → 写 ehentai_session.json

5. emit ehentai://login { cookie }

6. 关窗
```

## 6. macOS Cookie FFI（cookies.rs）

### 捕获顺序

1. Tauri `window.cookies()`（含 HttpOnly）
2. 各登录窗 webview dataStore
3. macOS 共享 WKWebsiteDataStore `defaultDataStore`
4. JS eval `about:blank#encodeURIComponent(document.cookie)`

### 原生 API

- ObjC 符号：`objc_getClass` / `objc_msgSend` / `sel_registerName` / `_NSConcreteStackBlock`
- 关键 selector：`getAllCookies:` / `count` / `objectAtIndex:` / `name` / `value` / `UTF8String`
- 遍历链：`configuration → websiteDataStore → httpCookieStore`
- 完成块：`Block_literal`（`_NSConcreteStackBlock` isa + `flags=0`，手动构建）

### 登出清理

- `deleteCookie:completionHandler:`（不是 `deleteCookie:`）
- 按域后缀匹配（小写、去前导点）：`dom == s || dom.ends_with(".{s}")`
- Pixiv 登出：`["pixiv.net"]`
- EHentai 登出：`["e-hentai.org", "exhentai.org"]`
- reset_app_data：三者全清

### 超时

2.5s（250 × 10ms）自旋等待。

## 7. Session 持久化

| 文件 | 内容 |
|---|---|
| `pixiv_session.json` | `{ cookie, user_id, user_name?, saved_at }` |
| `ehentai_session.json` | `{ cookie, saved_at }` |

- 启动时从 app data dir 恢复
- `set` 覆写
- 登出清空

## 8. 会话判定

- Pixiv：含 `PHPSESSID=`
- EHentai：含 `ipb_member_id=` 且 `ipb_pass_hash=`

## 9. 前端事件监听

- `pixiv://login` → `PixivDownload.vue` 更新登录态 + toast
- `ehentai://login` → `EHentai.vue` 更新登录态 + toast

## 10. 约束

- 登录窗 label 固定：`pixiv-login` / `ehentai-login`
- 捕获顺序不可变（HttpOnly cookie 必须通过原生 API）
- 登出必须精确按域清理，不误伤主窗口数据
- Windows/其他平台 cookie FFI 为 no-op

## 11. 相关模块

- [module-browse.md](./module-browse.md) — 浏览源（使用 cookie 鉴权）
- [module-task.md](./module-task.md) — 任务系统（入队时传入 cookie）
