# AGENTS.md

## 项目概述

EroLib（工口图书馆）—— Tauri 2 + Vue 3 本地漫画库管理器，下载源支持 Pixiv 与 EHentai。UI 用 Google Material Design 3 Web Components（@material/web）手搓。应用标识符 `im.xrl.erolib`。

## 架构分层

- **前端** `src/`：Vue 3 `<script setup>` + TS + Pinia + Vue Router。MWC 组件统一在 `src/material-web.ts` 注册（别直接引 `.js`）。
- **后端** `src-tauri/`（Rust）：命令在 `src-tauri/src/commands/`，业务在 `src-tauri/src/services/`，命令注册于 `main.rs` 的 `invoke_handler`。**新增命令必须同步 `src/services/api.ts`**。
- **serde 约定**：后端 struct 透传前端时务必 `#[serde(rename_all = "camelCase")]`，否则前端读不到字段（snake_case → undefined，是高频 bug 源）。

## 状态与持久化

- **localStorage**：主题/语言、阅读器缩放模式 `erolib.reader.zoomMode`、每书阅读进度 `erolib.reader.progress.{bookId}`、Pixiv tab `erolib.pixiv.tab`、Settings tab `erolib.settings.tab`。
- **IndexedDB**：书库封面低清缩略图缓存，见 `src/services/thumb-cache.ts`（DB `erolib`，store `thumbs`，key=bookId，value=Blob）。
- **Pinia store（内存，跨视图存活到退出）**：`stores/pixiv-browse.ts`（关注/收藏 feed、封面、卡片任务状态）、`stores/library.ts`（搜索/标签/结果）。
- **滚动位置**：五个二级页面（Library/Pixiv/EHentai/Tasks/Settings）的滚动都在 `App.vue` 的 `.app-main`（`overflow:auto`），其 scrollTop 按 `route.path` 持久化到 `localStorage`（`erolib.scroll.{path}`），切回时恢复；Reader（`/reader/:id`）全屏不参与。

## 下载与任务系统

- **所有下载统一经 TaskManager**（`src-tauri/src/services/task_manager.rs`），底层用 aria2，无进程内回退。任务 payload 是枚举 `TaskPayload`：`PixivBookmarks` / `PixivUserWorks` / `PixivSingleWork{cookie,work_id}` / `EhentaiGallery`。
- **任务模型**（`services/task.rs` `TaskSnapshot`）含 `speed`（实时下行速度 B/s）、`logs`（步骤日志 JSON 数组，上限 ~200 行）、`book_id`（完成后回填，前端一键跳阅读器）。`enqueue` 保留最新 **100 条**（先 `DELETE … NOT IN (SELECT … ORDER BY created_at DESC LIMIT 99)` 再插入）。
- aria2 进度：`wait_for_gid_with_progress` 轮询 `tell_status`，回调里 `set_progress(.., speed)` + `append_log`；成功后 `register_stored_book` → `set_book_id`。
- 前端 `stores/tasks.ts` 全局监听 `task://progress`（更新列表 + 书库刷新）与 `task://toast`（终态 toast）；`views/Tasks.vue` 左右分栏——运行中卡片右下角显示速度，详情 pane 显示步骤日志 / 创建完成时间 / 操作区。
- **动图（ugoira, illustType==2）**：`process_pixiv_ugoira` 拉 `ugoira_meta`（frames + originalSrc zip）→ 解压**原始 jpg 帧序列**直接进 cb7，**不二次编码**；逐帧延时存 `Book.delays`（DB JSON）。阅读器按延时定时播放循环——转换瞬时 / 加载快 / 无损 / 原分辨率。兼容旧 gif/apng 书。

## Pixiv 浏览

- 浏览式：关注 feed（`/ajax/follow_latest/illust?p=&mode=all`，**不带 user_id**，session 识别用户）+ 收藏（`/ajax/user/{id}/illusts/bookmarks?offset=&limit=`），懒加载 ~30/页（IntersectionObserver sentinel）。
- 封面防盗链：`i.pximg.net` 需 `Referer: https://www.pixiv.net/`，走后端代理 `pixiv_proxy_image`（前端 `<img>` 不能设 Referer）。
- 卡片三态：本地有→点进阅读器；下载中→遮罩 + SVG 环形进度（**别用 md-circular-progress determinate**，会卡）；未下载→标题左上红点。`task://progress` 在 **store 层**监听（跨视图存活，下载完成自动翻转卡片）。

## EHentai 浏览

- 浏览式（`stores/ehentai-browse.ts`）：关键词搜索 + 10 大分类 chip 多选并集（`cats` = OR of selected bits），EXHentai 开关（`store.ex`）切换 `e-hentai.org` / `exhentai.org` 域名；scraper 解析 HTML（`glthumb` data-src、`glink`）。`browse_status` 用 gid+token 归一化匹配本地书。
- 未登录时隐藏搜索框（`v-if="loggedIn"`）与 EXHentai switch（`v-show="loggedIn"`）。
- 卡片三态同 Pixiv（`components/EHentaiCard.vue`）；封面走 `ehentai_proxy_thumb`（防盗链）。

## 共享服务器（OPDS / RSS）

- `commands/server.rs`：axum 起 OPDS（5269）/ RSS（1269）HTTP 服务，`start_*` 幂等、返回 base_url；`ServerHandle` 持 watch channel 做优雅关闭。
- 监听 `0.0.0.0`（局域网全开放、无鉴权）；base_url 用 `local_lan_ip()`（connected UDP socket 取出口 IP）使 feed 内链接对其它设备可达。
- OPDS feed（`services/opds.rs`）+ RSS feed（`services/rss.rs`）都 `SELECT * FROM books`；`/download/:id`（OPDS + RSS 共用 `serve_download`）发整本、`/covers/:id` 发封面。
- 前端 `stores/settings.ts` 管 opds/rss 的 port/running/busy/url/error + `autoStartAll()`（`App.vue` onMounted 调，开机即跑）。

## 登录与 cookie 采取

- **Pixiv**：`commands/pixiv_login.rs` 开应用内浏览器，**不导航**到设置页，登录后直接 capture cookie；`services/pixiv.rs` `fetch_current_user_id` 先从 PHPSESSID `{user_id}_{secret}` 解析（零网络），失败再回退抓重定向。
- **EHentai**：`commands/ehentai.rs` 论坛账号登录窗口 + capture。
- **macOS cookie FFI**（`commands/cookies.rs`）：`WKHTTPCookieStore` 原生采取 / 删除；登出按版块 host 后缀（`pixiv.net` / `e-hentai.org`、`exhentai.org`）`clear_section_cookies`，**用 `deleteCookie:completionHandler:`**（不是 `deleteCookie:`，更不是对 dataStore 调），共享 `WKWebsiteDataStore` 下不误伤主窗口 localStorage。
- session 持久化到 app data dir（`pixiv_session.json` / `ehentai_session.json`）：启动恢复、set 覆写、登出清空。

## 书库与缩略图

- 书库封面走低清 thumb：后端 `get_book_cover_thumb`（`image` crate 降采样最长边 256px JPEG），前端先查 IndexedDB，miss 再取并缓存；原图 `get_book_cover` 给 OPDS/详情。
- 搜索框 text 匹配 title / author / **tags**；标签 chip 行并集(OR)过滤，计数随文本结果变（文本优先），上限 30，满 30 末尾加不可选 `…` chip。

## 阅读器

- 一级页面，无侧栏；进出强制暗黑模式（保存原模式退出恢复）。
- 缩放模式 `contain`/`fill` 用 **CSS class**（`.reader-image--fill` = absolute + cover；`--contain` = 100% + contain），**不要用 inline `:style` 绑定 object-fit**（低分辨率图会因元素=intrinsic 而留白）。gif（动图）单页，`<img>` 原生循环播放。

## 主题

- `src/services/md3-theme.ts` 由 seed + light/dark 生成 `--md-sys-color-*`；改后调 `applyMd3Theme(seed, mode)` 全局生效。

## 常见陷阱

- MWC 2.4.1 **没有** `md-card` / `md-top-app-bar` / `md-navigation-rail` / `md-tooltip` / `md-chip`，需用 token 手搓；`md-icon-button` 路径是 `@material/web/iconbutton/icon-button.js`。
- `md-outlined-text-field` 用 `:value` + `@input`；`md-switch` / `md-tabs` / `md-outlined-select` / `md-slider` 用 ref + `addEventListener` 并在卸载时清理（change 非 composed）。
- `md-slider` 别用 `:value` 单向绑定（拖动被写回覆盖）；`md-circular-progress` determinate 频繁更新会卡，环形进度改手搓 SVG。
- 图标用 `@mdi/js` 的 path；别把 Vue 组件作为 MWC 自定义元素的 slot 内容（升级时机不识别）。

## 开发命令

```bash
pnpm install            # 装依赖
pnpm tauri dev          # 开发（热重载）
npm run build           # 前端构建（vue-tsc 2.x 类型检查 + vite；TS 5.x 兼容已修）
pnpm tauri build        # 生产包（.app / .dmg / .exe / .msi）
```

> ⚠️ macOS 27 + rustc ≤1.96：release 下偶发 `can't find crate for <proc-macro>` 多为 feature-config 缓存损坏（**非** malformed Mach-O），`cargo clean` 即可；`Cargo.toml` 的 `[profile.release] debug = 2` 是历史防御，详见注释。
