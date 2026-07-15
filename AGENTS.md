# AGENTS.md

## 项目概述

EroLib（工口图书馆）—— Tauri 2 + Vue 3 本地漫画库管理器，下载源支持 Pixiv、EHentai / EXHentai、ASMHentai 与 NiceCat。UI 用 Google Material Design 3 Web Components（@material/web）手搓。应用标识符 `im.xrl.erolib`。

## 架构分层

- **前端** `src/`：Vue 3 `<script setup>` + TS + Pinia + Vue Router。MWC 组件统一在 `src/material-web.ts` 注册（别直接引 `.js`）。
- **后端** `src-tauri/`（Rust）：命令在 `src-tauri/src/commands/`，业务在 `src-tauri/src/services/`，命令注册于 `main.rs` 的 `invoke_handler`。**新增命令必须同步 `src/services/api.ts`**。
- **serde 约定**：后端 struct 透传前端时务必 `#[serde(rename_all = "camelCase")]`，否则前端读不到字段（snake_case → undefined，是高频 bug 源）。

## 状态与持久化

- **SQLite 库文件** `data_dir/erolib.db`（Tauri `app_local_data_dir`，bundle id `im.xrl.erolib`）。启动时若旧 `manga-manager.db` 存在则连同 `-wal`/`-shm` 原子重命名迁移，绝不覆盖已存在的 `erolib.db`。
- **localStorage**：主题/语言、阅读器缩放模式 `erolib.reader.zoomMode`、每书阅读进度 `erolib.reader.progress.{bookId}`、Pixiv tab `erolib.pixiv.tab`、Settings tab `erolib.settings.tab`。
- **IndexedDB**：书库封面低清缩略图缓存，见 `src/services/thumb-cache.ts`（DB `erolib`，store `thumbs`，key=bookId，value=Blob）。
- **Pinia store（内存，跨视图存活到退出）**：`stores/pixiv-browse.ts`（关注/收藏 feed、封面、卡片任务状态）、`stores/library.ts`（搜索/标签/结果）。
- **滚动位置**：五个二级页面（Library/Pixiv/EHentai/Tasks/Settings）的滚动都在 `App.vue` 的 `.app-main`（`overflow:auto`），其 scrollTop 按 `route.path` 持久化到 `localStorage`（`erolib.scroll.{path}`），切回时恢复；Reader（`/reader/:id`）全屏不参与。

## 下载与任务系统

- **所有下载统一经 TaskManager**（`src-tauri/src/services/task_manager.rs`），无进程内回退。任务 payload 是枚举 `TaskPayload`：`PixivBookmarks` / `PixivUserWorks` / `PixivSingleWork{cookie,work_id}` / `EhentaiGallery` / `AhentaiGallery` / `NicecatGallery`。`TaskSource` 对应为 `Pixiv` / `Ehentai` / `Ahentai` / `Nicecat`。
- **下载后端**：Pixiv / EHentai 走 aria2（`Aria2Client`，8 路并发 gid，带字节级进度 + 平滑速度）；ASMHentai / NiceCat 走 8 路并发 reqwest 直连（`JoinSet` + `Semaphore(8)`，`add_bytes` + `set_speed` 实时追踪）。两种都进同一个 `TaskManager`，共享暂停 / 取消 / 进度语义。
- **aria2 自动 HTTP 代理**：`services/proxy.rs` `detect_http_proxy()` 取 env（`ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY` + 小写）+ macOS `scutil --proxy`（Clash / V2Ray「设为系统代理」后写入系统配置），结果 60s 缓存（避免一本几十张图每张都 spawn scutil）；跳过 aria2 不支持的 SOCKS。`Aria2Client::add_uri` 检测到则注入 `all-proxy` option，Pixiv / EHentai 等翻墙下载零配置。
- **任务模型**（`services/task.rs` `TaskSnapshot`）含 `speed`（实时下行速度 B/s）、`logs`（步骤日志 JSON 数组，上限 ~200 行）、`book_id`（完成后回填，前端一键跳阅读器）。`enqueue` 保留最新 **100 条**（先 `DELETE … NOT IN (SELECT … ORDER BY created_at DESC LIMIT 99)` 再插入）。
- aria2 进度：`wait_for_gid_with_progress` 轮询 `tell_status`，回调里 `set_progress(.., speed)` + `append_log`；成功后 `register_stored_book` → `set_book_id`。
- 前端 `stores/tasks.ts` 全局监听 `task://progress`（更新列表 + 书库刷新）与 `task://toast`（终态 toast）；`views/Tasks.vue` 左右分栏——运行中卡片右下角显示速度，详情 pane 显示步骤日志 / 创建完成时间 / 操作区。
- **动图（ugoira, illustType==2）**：`process_pixiv_ugoira` 拉 `ugoira_meta`（frames + originalSrc zip）→ 解压**原始 jpg 帧序列**直接进 cb7，**不二次编码**；逐帧延时存 `Book.delays`（DB JSON）。阅读器按延时定时播放循环——转换瞬时 / 加载快 / 无损 / 原分辨率。兼容旧 gif/apng 书。

## Pixiv 浏览

- 浏览式：推荐 feed（`/ajax/top/illust?mode=all`，一次性拉取，**不分页**）+ 关注 feed（`/ajax/follow_latest/illust?p=&mode=all`，**不带 user_id**，session 识别用户）+ 收藏（`/ajax/user/{id}/illusts/bookmarks?tag=&offset=&limit=&rest=show`）+ 关键词搜索（`/ajax/search/artworks/{keyword}?word=&mode=all&s_mode=s_tag&type=all&order=date_d&p=`，`p` 分页）。四个 feed 共用 `body.thumbnails.illust` 结构。
- 封面防盗链：`i.pximg.net` 需 `Referer: https://www.pixiv.net/`，走后端代理 `pixiv_proxy_image`（前端 `<img>` 不能设 Referer）。
- 卡片三态：本地有→点进阅读器；下载中→遮罩 + SVG 环形进度（**别用 md-circular-progress determinate**，会卡）；未下载→标题左上红点。`task://progress` 在 **store 层**监听（跨视图存活，下载完成自动翻转卡片）。

## EHentai 浏览

- 浏览式（`stores/ehentai-browse.ts`）：关键词搜索 + 10 大分类 chip 多选并集（`cats` = OR of selected bits），EXHentai 开关（`store.ex`）切换 `e-hentai.org` / `exhentai.org` 域名；scraper 解析 HTML（`glthumb` data-src、`glink`）。`browse_status` 用 gid+token 归一化匹配本地书。
- 未登录时隐藏搜索框（`v-if="loggedIn"`）与 EXHentai switch（`v-show="loggedIn"`）。
- 卡片三态同 Pixiv（`components/EHentaiCard.vue`）；封面走 `ehentai_proxy_thumb`（防盗链）。

## 共享服务器（OPDS / RSS）

- `commands/server.rs`：axum 起 OPDS（5269）/ RSS（1269）HTTP 服务，`start_*` 幂等、返回 base_url；`ServerState`（`Clone` struct）持 `db / storage / covers_path / base_url / rss_service / opds_service`（共享已配置的 service，替代原先每处理器 `::new(db)` 重建，避免 base_url / handler 不一致）；watch channel 做优雅关闭。
- **直出图片**：新增 `/pages/:id/:n`（`serve_page`，单页图，复用 cb7 归档内存缓存 + `guess_image_mime` 嗅探 MIME）+ `/article/:id`（`serve_article`，整本 HTML 画廊，`<img src="{base}/pages/{id}/{n}">` 逐页）；注册进 OPDS + RSS 两个 router。RSS 阅读器可**直接翻看整本图片序列**，不再只给下载链接。
- **富摘要**（`services/feed.rs` 抽出共享 `book_metadata_blurb`：作者｜页数·格式·大小｜标签｜来源｜链接｜原始文件｜发布日期｜阅读记录）：RSS `<description>` + `<content:encoded>`（全页 `<img>` 条）与 OPDS `<summary type="html">` 统一复用；feed 标题统一 `EroLib`（非 "EroLib Library"）。
- 监听 `0.0.0.0`（局域网全开放、无鉴权）；base_url 用 `local_lan_ip()`（connected UDP socket 取出口 IP）使 feed 内链接对其它设备可达。`/download/:id`（`serve_download`）发整本、`/covers/:id` 发封面。
- 前端 `stores/settings.ts` 管 opds/rss 的 port/running/busy/url/error + `autoStartAll()`（`App.vue` onMounted 调，开机即跑）。

## ASMHentai 浏览

- 无需登录，公开站点 `asmhentai.com`；浏览式（`stores/ahentai-browse.ts`）：关键词搜索，无 tab 无分类 chip，48 条/前端页（源站 20 条/页缓冲）。
- listing 页仅提取 id / title / thumb_url / category；`page_count` 始终 0、`uploader` 始终 None（节省带宽简化设计）。下载时才通过 `fetch_gallery_meta` 抓详情页补全标签 / 作者 / 页数，并用 `strip_tag_count()` 清洗 tag 名末尾的 ` (12,345)` 计数后缀。
- 卡片两态（无页码 badge、无作者副标题）：本地有 → 阅读器；未下载 → 入队下载；下载中走 `task://progress` 监听。
- 图片 CDN：`images.asmhentai.com/{load_dir}/{id}/{page}.jpg`；封面走 `ahentai_proxy_thumb` 代理 + IndexedDB 缓存。
- 任务标题统一 `ASMHentai: {title}`；`process_ahentai` 走 JoinSet + Semaphore(8) 并发下载，含 `add_bytes` + `set_speed` 实时追踪。

## NiceCat 浏览

- 无需登录，公开站点 `ncmm.cc`；**全程纯 HTTP，无需内嵌 WebView**——通过 RC4 动态令牌直连 `gxxa.fun` API（`services/nicecat.rs` `NicecatApiClient`）。
- 浏览式（`stores/nicecat-browse.ts`）：首页话题板块（`HomeFeed/randomFeed`，横向滚动）+ 关键词搜索。搜索走两阶段——`ComicSearch/search` 解析关键词到多个标签，取 `comic_number` 最大的单一最优标签，再 `ComicSearch/searchTag` 取结果页 + `searchId` 游标；前端回传 `cursor` 字段翻页（空 = 页 1，非空 = 推进游标），页粒度 60，游标耗尽或短页即 end。
- 卡片三态同 Pixiv / EHentai（`components/SourceCard` + `useBrowseFeed` 复用）：本地有 → 阅读器；下载中 → 遮罩 + 环形进度；未下载 → 红点。`nicecat_browse_status` 按 `source_plugin='nicecat'` + `source_post_id` 归一化匹配本地书与在途任务。
- 封面走 `nicecat_proxy_thumb` 代理绕过 `vurm.fun` CDN 防盗链（需 `Referer` + `Origin`）。
- **下载**（`process_nicecat`，`task_manager.rs`）：纯 HTTP——并发拉 `ComicInfo/info`（元信息：标题 / 作者 / 标签）+ `ComicOrder/getComicOrder`（翻页图 URL，需当日 `dateKey` = Base64(SHA-256(本地午夜毫秒))），解析出页图 URL 后 8 路并发 reqwest 直连 `vurm.fun` 下载，打包 cb7。**无 WebView**。
- 任务标题 `NiceCat: {title}`；payload `NicecatGallery { comic_id, title }`，`TaskSource::Nicecat`。
- **RC4 令牌**：每请求新随机 token（一次性，复用 403）。`generate_token` = Base64(RC4(key, JSON({uid, auth})))，key `Zo1Eq4V2mr269K4doL9U4093U25acjMQ`，auth `ec8be430bc634535b258b3591a414a67`（`nicecat.rs`）。

## 登录与 cookie 采取

- **Pixiv**：`commands/pixiv_login.rs` 开应用内浏览器，**不导航**到设置页，登录后直接 capture cookie；`services/pixiv.rs` `fetch_current_user_id` 先从 PHPSESSID `{user_id}_{secret}` 解析（零网络），失败再回退抓重定向。
- **EHentai**：`commands/ehentai.rs` 论坛账号登录窗口 + capture。
- **macOS cookie FFI**（`commands/cookies.rs`）：`WKHTTPCookieStore` 原生采取 / 删除；登出按版块 host 后缀（`pixiv.net` / `e-hentai.org`、`exhentai.org`）`clear_section_cookies`，**用 `deleteCookie:completionHandler:`**（不是 `deleteCookie:`，更不是对 dataStore 调），共享 `WKWebsiteDataStore` 下不误伤主窗口 localStorage。
- session 持久化到 app data dir（`pixiv_session.json` / `ehentai_session.json`）：启动恢复、set 覆写、登出清空。

## 书库与缩略图

- 书库封面走低清 thumb：后端 `get_book_cover_thumb`（`image` crate 降采样最长边 256px JPEG），前端先查 IndexedDB，miss 再取并缓存；原图 `get_book_cover` 给 OPDS/详情。
- 搜索框 text 匹配 title / author / **tags**；标签 chip 行并集(OR)过滤，计数随文本结果变（文本优先），上限 30，满 30 末尾加不可选 `…` chip。

## 首页与阅读时长追踪

- 首页 `views/Home.vue`（路由 `/`）：Hero 区显示 **本周阅读时长**（`get_weekly_reading_ms`，按周一起点 Monday 00:00 local 归属，跨周一致不重不漏）+ 库封面 **旋转墙**（`WallCover`，21 本旋转展示）；下方「最近阅读」书架（`list_recent_books` 按 `last_read_at` 降序，点卡片进阅读器）。
- `reading_sessions` 表追踪每次阅读会话（id / book_id / started_at / ended_at / duration_ms）。`open_book` 开会话（同时 bump `last_read_at` + `read_count`）并返回 session id；`record_reading(session_id)` 收尾，写 `ended_at` + 本次 `duration_ms`。
- **前端 `Reader.vue` 增量上报**：每 tick 上报**本次会话增量**——`readTimeSessionBaseline` 在会话开启时快照 `readTimeAccumulated`（该书历史累计），delta = 累计 − baseline；仅前台（`document.hidden === false`）累计，后台不计时。`close_stale_sessions` 在启动收尾遗留的 `ended_at IS NULL` 行（duration=0）。
- **`open_book` 取 id 必须用 `INSERT … RETURNING id`**：`last_insert_rowid()` 是连接级，sqlx 连接池（max_connections=8）两条语句跨连接会返回别的连接上一次插入的 rowid（或 0）→ `record_reading` 的 `WHERE id = ?` 永远命不中真正那行 → 每会话 `duration_ms` 恒 0、首页统计恒 0（已踩坑，见 memory `sqlx-pool-last-insert-rowid`）。
- 历史污染修复：`user_version=1` 一次性清零被污染的历史 `duration_ms`（旧前端每秒推累计而非增量）。

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
