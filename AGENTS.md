# AGENTS.md

## 项目概述

EroLib（工口图书馆）—— Tauri 2 + Vue 3 本地漫画库管理器，下载源支持 Pixiv、EHentai / EXHentai、ASMHentai 与 NiceCat。UI 用 Google Material Design 3 Web Components（@material/web）手搓。应用标识符 `im.xrl.erolib`。

## 架构分层

- **前端** `src/`：Vue 3 `<script setup>` + TS + Pinia + Vue Router。MWC 组件统一在 `src/material-web.ts` 注册（别直接引 `.js`）。
- **后端** `src-tauri/`（Rust）：命令在 `src-tauri/src/commands/`，业务在 `src-tauri/src/services/`，命令注册于 `main.rs` 的 `invoke_handler`。**新增命令必须同步 `src/services/api.ts`**。
- **serde 约定**：后端 struct 透传前端时务必 `#[serde(rename_all = "camelCase")]`，否则前端读不到字段（snake_case → undefined，是高频 bug 源）。

## 状态与持久化

### 概览

三层存储格局：**SQLite**（后端核心，权威数据）→ **文件系统**（CB7 书档 + 封面原图）→ **localStorage / IndexedDB**（前端 UI 偏好 + 封面缩略图缓存）。另外 macOS WKWebView 原生 Cookie Store 承担登录态持久化。

### SQLite — `erolib.db`

**位置**：`<app_local_data_dir>/erolib.db`（bundle id `im.xrl.erolib`，macOS 上为 `~/Library/Application Support/im.xrl.erolib/erolib.db`）。

**引擎**：sqlx 0.7 + rusqlite，WAL 模式，8 连接池，`busy_timeout=5000`，`foreign_keys=ON`。启动时 `schema/schema.sql` 全量执行，所有 DDL 均为 `CREATE IF NOT EXISTS`，幂等。

启动时若旧 `manga-manager.db` 存在则连同 `-wal`/`-shm` 原子重命名迁移，绝不覆盖已存在的 `erolib.db`。

| 表 | 主键 | 行数级 | 用途 |
|---|---|---|---|
| `books` | `id TEXT` (UUID) | ~100-1000 | 每本书一行（title, file_path, page_count, source_plugin/url/post_id, author, published_at, delays 等 20 列） |
| `tags` | `id TEXT` (UUID) | ~500 | 标签词典，`name UNIQUE`，`tag_type` 默认 `'custom'` |
| `book_tags` | `(book_id, tag_id)` PK + FK CASCADE | ~1500 | 多对多书-标签关联 |
| `collections` / `collection_books` | UUID | 0-100 | 阅读列表（name, position, description）+ 多对多书-列表关联，`position` 决定了排序 |
| `tasks` | `id TEXT` (UUID) | ~0-50 | 下载任务队列（status, progress, logs JSON, payload JSON, retry_count） |
| `reading_sessions` | `id INTEGER AUTOINCREMENT` | 增长中 | 阅读时长追踪（book_id, started_at, ended_at, duration_ms） |
| `books_fts` | FTS5 虚拟表 | 与 books 同步 | 全文索引（title, original_filename, tags），`porter unicode61` 分词，三触发器自动同步 |

### 文件系统 — CB7 书档 + 封面

**目录结构**（均在 `<app_local_data_dir>/` 下）：

```
library/       {uuid}.cb7                  # ZIP 容器 (ComicInfo.xml + 图片页)
covers/        {book_id}.{jpg|png|webp}    # 提取的首图封面
downloads/     {task_id}/                   # 下载中的临时页文件（aria2 输出）
pixiv_session.json                          # Pixiv 登录凭证 JSON
ehentai_session.json                        # EHentai 登录凭证 JSON
```

**CB7**：`create_cb7` 写入 ZIP（含 `ComicInfo.xml` 元信息 + `0001.jpg` 编号图片页）。`extract_cover` 从 CB7 提取首图写入 `covers/`。`PAGE_CACHE_MAX=8` 内存缓存已打开的 CB7 以避免重复扫描 ZIP 中央目录。

### 登录凭证 — JSON 文件 + WKWebView Cookie Store（macOS 双层）

| 层 | 存储位置 | 操作方式 |
|---|---|---|
| 应用级 JSON | `pixiv_session.json` / `ehentai_session.json` | `serde_json` 读写，含 `{cookie, saved_at}` |
| 系统级 Cookie | WKWebView `WKHTTPCookieStore` | ObjC FFI `getAllCookies:` / `deleteCookie:completionHandler:` |

登录时从 WebView 捕获 cookie 双写；登出时 JSON 文件 `remove_file` + WKWebView 精确 `deleteCookie`（按域名 match，不禁用 `clear_all_browsing_data` 以免误伤他站）；`reset_app_data` 全部删除。

### localStorage — 14 个键模式（前端 UI 偏好）

| 键 | 来源 | 数据 |
|---|---|---|
| `erolib.scroll.*` | `App.vue` | 按 `route.path` 的滚动位置 scrollTop |
| `erolib.seed` | `stores/theme.ts` | MD3 种子色（`pink\|violet\|blue\|teal`） |
| `erolib.theme` | `stores/theme.ts` | 明暗模式（`light\|dark`） |
| `erolib.locale` | `i18n/index.ts` | 界面语言（`zh\|en\|ja`） |
| `erolib.opdsPort` | `stores/settings.ts` | OPDS 服务器端口（默认 `5269`） |
| `erolib.rssPort` | `stores/settings.ts` | RSS 服务器端口（默认 `1269`） |
| `erolib.localSyncEnabled` | `stores/settings.ts` | 本地同步开关（`'1'`/`'0'`） |
| `erolib.localSyncDir` | `stores/settings.ts` | 本地同步目标目录路径 |
| `erolib.ehentai.ex` | `stores/ehentai-browse.ts` | EXHentai 模式开关 |
| `erolib.reader.zoomMode` | `views/Reader.vue` | 缩放模式（`fill\|contain`） |
| `erolib.reader.progress.*` | `views/Reader.vue` | 按 `bookId` 的阅读页码 |
| `erolib.reader.readtime.*` | `views/Reader.vue` | 按 `bookId` 的累计阅读秒数 |
| `erolib.settings.tab` | `views/Settings.vue` | 设置页当前 tab |
| `erolib.pixiv.tab` | `views/PixivDownload.vue` | Pixiv 页当前 tab |

### IndexedDB — 封面缩略图缓存（`erolib` / v1 / `thumbs`）

**库**：`idb` ^8.0.0，文件 `src/services/thumb-cache.ts`。key = `bookId`（`source_post_id \|\| book.id`），value = 低清 JPEG Blob（最宽 256px）。

**读链**：`useBrowseFeed.loadCover()` / `Library.vue.loadCover()` 先 `getThumb(key)` → 命中则 `URL.createObjectURL(blob)` 直接给 `<img>`；miss 则 IPC 取后端缩略图 → 回填 IndexedDB。

**性能优势（vs 文件系统直读）**：缩略图读链路若走后端 IPC（`invoke('get_book_cover_thumb')` → Rust `fs::read` → image crate resize → base64 序列化 → JSON-RPC 传输 → JS 反序列化 → Blob 构造），单张约 3-6ms（IPC 往返 + 序列化占主要开销）。IndexedDB 读链纯浏览器进程内，无 IPC 无序列化，单张约 0.2ms。书库页面一次性加载 48 张封面时，IndexedDB ~10ms（无缝），IPC 方案 ~192ms（用户可感知的闪烁）。**结论：IndexedDB 是最优选择，速度为核心考量，维持不变。**

### Pinia Store（内存，跨视图存活到退出）

| Store | 持久化支撑 | 用途 |
|---|---|---|
| `theme` | localStorage seed + theme | 全局配色 |
| `settings` | localStorage opdsPort/rssPort/localSync* | 服务器端口 + 同步配置 |
| `ehentai-browse` | localStorage ex + IndexedDB covers | E 站 EX 模式 + 浏览源/卡片状态/封面 |
| `pixiv-browse` | IndexedDB covers（通过 `useBrowseFeed`） | 关注/收藏 feed、卡片任务状态、封面 |
| `ahentai-browse` | IndexedDB covers（通过 `useBrowseFeed`） | ASMHentai 浏览源/状态/封面 |
| `nicecat-browse` | IndexedDB covers（通过 `useBrowseFeed`） | NiceCat 浏览源/状态/封面 |
| `library` | IndexedDB covers（在 `Library.vue` 协调） | 书籍列表、搜索查询、标签过滤、分页 |
| `collections` | SQLite `collections` / `collection_books` 表（后端持久化） | 阅读列表（创建/重命名/删除/重排/加书/移书），activeCollectionId 不跨重启持久化 |
| `tasks` | 无客户端持久化（后端事件流驱动） | 任务列表、选中任务、进度 |
| `toast` | 无持久化 | 4 秒自动消失的提示消息 |

### 代理检测（不持久化）

`services/proxy.rs` `detect_http_proxy()`：环境变量（`ALL_PROXY`/`HTTPS_PROXY`/`HTTP_PROXY` + 小写变体）+ macOS `scutil --proxy` 系统代理设置。结果进 `tokio::sync::Mutex` 缓存 60s（避免每张图 spawn scutil），aria2 `all-proxy` option 注入 SOCKS 自动跳过。

## 下载与任务系统

- **所有下载统一经 TaskManager**（`src-tauri/src/services/task_manager.rs`），无进程内回退。任务 payload 是枚举 `TaskPayload`：`PixivBookmarks` / `PixivUserWorks` / `PixivSingleWork{cookie,work_id}` / `EhentaiGallery` / `AhentaiGallery` / `NicecatGallery`。`TaskSource` 对应为 `Pixiv` / `Ehentai` / `Ahentai` / `Nicecat`。
- **下载后端**：四个 source 统一走 **分批并发下载**（`download_pages_concurrent`，8 并发/批，批间检查取消/暂停，图片落临时目录供断点续传），Pixiv / EHentai 走 aria2，ASMHentai / NiceCat 走 reqwest 直连（`JoinSet` + `Semaphore(8)`，`add_bytes` + `set_speed` 实时追踪）。进同一个 `TaskManager`，共享暂停 / 取消 / 进度语义。
- **aria2 自动 HTTP 代理**：`services/proxy.rs` `detect_http_proxy()` 取 env（`ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY` + 小写）+ macOS `scutil --proxy`（Clash / V2Ray「设为系统代理」后写入系统配置），结果 60s 缓存（避免一本几十张图每张都 spawn scutil）；跳过 aria2 不支持的 SOCKS。`Aria2Client::add_uri` 检测到则注入 `all-proxy` option，Pixiv / EHentai 等翻墙下载零配置。
- **任务模型**（`services/task.rs` `TaskSnapshot`）含 `speed`（实时下行速度 B/s）、`logs`（步骤日志 JSON 数组，上限 ~200 行）、`book_id`（完成后回填，前端一键跳阅读器）。`enqueue` 保留最新 **100 条**（先 `DELETE … NOT IN (SELECT … ORDER BY created_at DESC LIMIT 99)` 再插入）。
- aria2 进度：`wait_for_gid_with_progress` 轮询 `tell_status`，回调里 `set_progress(.., speed)` + `append_log`；成功后 `register_stored_book` → `set_book_id`。
- 前端 `stores/tasks.ts` 全局监听 `task://progress`（更新列表 + 书库刷新）与 `task://toast`（终态 toast）；`views/Tasks.vue` 左右分栏——运行中卡片右下角显示速度，详情 pane 显示步骤日志 / 创建完成时间 / 操作区。
- **分批下载**（`download_pages_concurrent`）：所有 source 统一走 8 并发 JoinSet，每页一条日志 `📥 第 i/n 页 完成`，错误日志 `❌ 第 i 页失败`；图片落临时目录供断点续传；每页完成后推送进度。
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
- 阅读列表（collections）：右侧抽屉面板管理，右键列表名直接内联重命名（input 与文字同款外观，仅光标闪烁提示）；重命名期间底部 `+` 变为红色垃圾桶按钮，点击弹出 `md-dialog` 确认删除。创建列表自动去重命名（`新的阅读列表 1/2/...`），上限 100。选中列表后书库标题变为 `"列表名"`。

## 首页与阅读时长追踪

- 首页 `views/Home.vue`（路由 `/`）：Hero 区显示 **本周阅读时长**（`get_weekly_reading_ms`，按周一起点 Monday 00:00 local 归属，跨周一致不重不漏）+ 库封面 **旋转墙**（`WallCover`，21 本旋转展示）；下方「最近阅读」书架（`list_recent_books` 按 `last_read_at` 降序，点卡片进阅读器）。
- 删除最近阅读书籍后重新拉取 12 本补位，新书籍立刻载入缩略图，删除成功/失败均有 toast。
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
- **全局 `user-select: none` + `:focus-visible { outline: none }`**（`styles/md3.css` 全局默认）：拖选/MWC 焦点蓝框全部禁用。输入框/textarea/[contenteditable] 例外保持选中 + 光标。**不要在组件里单独加 `user-select` 或 `outline`**——全局规则已覆盖，组件级覆盖反而产生不一致。

## 开发命令

```bash
pnpm install            # 装依赖
pnpm tauri dev          # 开发（热重载）
npm run build           # 前端构建（vue-tsc 2.x 类型检查 + vite；TS 5.x 兼容已修）
pnpm tauri build        # 生产包（.app / .dmg / .exe / .msi）
```

> ⚠️ macOS 27 + rustc ≤1.96：release 下偶发 `can't find crate for <proc-macro>` 多为 feature-config 缓存损坏（**非** malformed Mach-O），`cargo clean` 即可；`Cargo.toml` 的 `[profile.release] debug = 2` 是历史防御，详见注释。
