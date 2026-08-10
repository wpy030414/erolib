# ARCHITECTURE — EroLib 架构地图

> 架构地图：描述稳定的结构关系，让读者理解系统边界。通常半年甚至一年不变。

## 1. 系统全景

```
┌─────────────────────────────────────────────────────────────┐
│                      Tauri 2 Desktop App                     │
│                                                             │
│  ┌─────────────────────────┐   ┌──────────────────────────┐ │
│  │     Frontend (Vue 3)     │   │     Backend (Rust)       │ │
│  │                         │   │                          │ │
│  │  views/ stores/         │   │  commands/  services/    │ │
│  │  components/            │◄─►│  db/  models/            │ │
│  │  composables/           │IPC│                          │ │
│  │  services/ i18n/        │   │  TaskManager (aria2)     │ │
│  │                         │   │  OPDS/RSS (axum)         │ │
│  └─────────────────────────┘   └──────────┬───────────────┘ │
│                                            │                 │
│  ┌──────────────┐   ┌──────────────────────┴───────────────┐ │
│  │  IndexedDB    │   │          SQLite (WAL)                │ │
│  │  (封面缩略图)  │   │  books tags collections tasks ...    │ │
│  └──────────────┘   └──────────────────────────────────────┘ │
│                                                             │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │              File System (app_local_data_dir)            │ │
│  │  library/*.cb7  covers/*.jpg  downloads/  aria2/         │ │
│  └──────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**分层原则**：
- 前端只管 UI 状态和用户交互，不直接操作文件或网络
- 后端是唯一的 I/O 层（文件读写、网络请求、数据库操作）
- 前后端通过 68 个 Tauri `invoke` 命令 + 6 个事件通道通信

## 2. 前端架构

```
src/
├── views/          页面（Home / Library / Reader / Pixiv / EHentai / AHentai / NiceCat / Tasks / Settings）
├── stores/         Pinia stores + 模块级响应式（theme / settings / library / tasks / toast / update / collections / *-browse）
├── components/     共享组件（AppShell / SourceCard / FeedList / SearchBox / WallCover / CollectionDialog / ...）
├── composables/    可复用逻辑（useBrowseFeed / useInfiniteSentinel / useDebouncedModel）
├── services/       API 封装（api.ts）+ MD3 主题引擎（md3-theme.ts）+ IndexedDB 封面缓存（thumb-cache.ts）
├── i18n/           三语字典（zh / en / ja，240 键）
├── types/          TypeScript 接口（Book / TaskItem / CardStatus / PixivWork / GalleryListItem / ...）
├── styles/         MD3 design tokens（tokens.css）+ 全局基础样式（md3.css）
└── router/         Vue Router 路由定义（hash mode）
```

### 路由

所有路由均无 `meta`，使用 `createWebHashHistory()`：

| 路径 | 视图 | 备注 |
|---|---|---|
| `/` | — | redirect → `/home` |
| `/home` | `Home.vue` | 首页 |
| `/library` | `Library.vue` | 书库 |
| `/reader/:id` | `Reader.vue` | 阅读器（props: true） |
| `/pixiv` | `PixivDownload.vue` | Pixiv 浏览/下载 |
| `/ehentai` | `EHentai.vue` | EHentai 浏览 |
| `/ahentai` | `AHentai.vue` | AHentai 浏览 |
| `/nicecat` | `NiceCat.vue` | NiceCat 浏览 |
| `/tasks` | `Tasks.vue` | 任务列表 |
| `/settings` | `Settings.vue` | 设置 |

### 数据流

```
用户操作 → view → store → api.ts → invoke('command_name') → 后端
后端 emit 事件 → store 监听 → 响应式更新 → view 重渲染
```

### 浏览源通用架构（useBrowseFeed）

四个浏览源（Pixiv / EHentai / AHentai / NiceCat）共享同一套基础设施：

```
useBrowseFeed<TItem, TKey, TStatus, TCursor>
├── feed: { items, loading, end }      — 分页状态
├── coverMap: Record<key, blobURL>     — 封面临时 URL
├── statusMap: Record<key, CardStatus> — 卡片三态
├── loadMore()                         — 跨源缓冲到 48 条/页
├── loadCover(item)                    — IndexedDB → proxy → Blob（6 并发上限）
└── 事件监听（listen=true 时 armed）
    ├── task://progress → patch statusMap
    └── book://deleted → 清除 localBookId
```

Pixiv 的 4 个 feed（recommend / following / bookmark / search）共享同一个 `coverMap / statusMap / coverLoading`（同一作品跨 tab 状态一致）。其余源各自独立。

### localStorage 键清单

| 键 | 来源 | 数据 |
|---|---|---|
| `erolib.scroll.*` | `App.vue` | 按 `route.path` 的滚动位置 scrollTop |
| `erolib.seed` | `stores/theme.ts` | MD3 种子色（`pink\|violet\|blue\|teal\|custom:<uuid>`） |
| `erolib.theme` | `stores/theme.ts` | 明暗模式（`light\|dark`） |
| `erolib.customThemes` | `stores/theme.ts` | 自定义主题 JSON（`Record<string, CustomTheme>`，上限 3） |
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

### 事件通道

| 事件名 | 方向 | 载荷 | 监听者 |
|---|---|---|---|
| `task://progress` | 后端 → 前端 | `TaskSnapshot` | `stores/tasks.ts`、`useBrowseFeed`（listen=true） |
| `task://toast` | 后端 → 前端 | `{ kind, title }` | `stores/tasks.ts` |
| `book://deleted` | 后端 → 前端 | `{ bookId }` | `stores/tasks.ts`、`useBrowseFeed`（listen=true） |
| `pixiv://login` | 后端 → 前端 | `{ user_id, cookie, user_name? }` | `views/PixivDownload.vue` |
| `ehentai://login` | 后端 → 前端 | `{ cookie }` | `views/EHentai.vue` |
| `update://progress` | 后端 → 前端 | `UpdateProgress` | `stores/update.ts` |

## 3. 后端架构

```
src-tauri/src/
├── main.rs           命令注册入口 + AppState 构建 + 启动迁移
├── commands/         Tauri 命令层（薄，参数校验 + 调 service + map_err）
│   ├── book.rs       13 命令（CRUD + 阅读会话 + 统计）
│   ├── pixiv.rs      9 命令（登录读写清 + 浏览 + 代理）
│   ├── pixiv_login.rs 1 命令（开登录窗 + 轮询 cookie）
│   ├── ehentai.rs    6 命令（登录 + 搜索 + 代理）
│   ├── ahentai.rs    3 命令（搜索 + 代理）
│   ├── nicecat.rs    3 命令（代理 + API 透传）
│   ├── tasks.rs      12 命令（CRUD + 四种源入队）
│   ├── collection.rs 8 命令（阅读列表 CRUD）
│   ├── server.rs     4 命令（OPDS/RSS 启停）
│   ├── search.rs     2 命令（全文搜索 + 标签计数）
│   ├── update.rs     4 命令（检查 + 下载 + 安装 + 退出安装）
│   ├── sync.rs       1 命令（单向本地同步）
│   ├── reset.rs      1 命令（清空全部数据）
│   ├── settings.rs   1 命令（设置语言）
│   └── cookies.rs    工具模块（macOS WKHTTPCookieStore FFI，无命令暴露）
├── services/         业务逻辑层
│   ├── task_manager.rs 核心任务调度（~2500 行）
│   ├── task.rs       数据模型（TaskSource / TaskStatus / TaskPayload / Task / TaskSnapshot）
│   ├── aria2.rs      aria2 JSON-RPC 客户端 + 本地 daemon 启动
│   ├── proxy.rs      系统 HTTP 代理自动检测（env + scutil，60s 缓存）
│   ├── pixiv.rs      Pixiv API 客户端
│   ├── ehentai.rs    EHentai HTML 解析客户端
│   ├── ahentai.rs    ASMHentai HTML 解析客户端
│   ├── nicecat.rs    NiceCat HTTP API 客户端（RC4 令牌鉴权）
│   ├── library.rs    书库核心（导入 / 删除 / 阅读会话 / 统计）
│   ├── collection_service.rs 阅读列表服务
│   ├── search.rs     搜索服务（LIKE + 标签本地化合并）
│   ├── similarity.rs SIMILARITY UDF（Levenshtein 归一化）
│   ├── locale.rs     三语标签翻译物化
│   ├── storage.rs    CB7 文件管理（create_cb7 / extract_cover / read_page / 缓存）
│   ├── opds.rs       OPDS Atom feed 生成
│   ├── rss.rs        RSS 2.0 feed 生成
│   └── feed.rs       OPDS / RSS 共享的元信息渲染（book_metadata_blurb）
├── db/mod.rs         SQLite 连接池 + schema 启动
├── models/mod.rs     Rust 数据模型（Book / Tag / TagCount / Collection / BookMetadata / SearchQuery / SearchResult）
└── errors.rs         AppError 枚举（thiserror → IntoResponse）
```

### 启动序列（main.rs）

```
1. tracing_subscriber 初始化（env-filter: info）
2. Tauri Builder setup:
   a. Database::new → 打开 erolib.db（WAL / 8 连接 / busy_timeout=5000）
      → 执行 schema.sql + tag_translations.sql（幂等）
      → materialize_form_map + materialize_resolved
   b. StorageService::new → 创建 library/ + covers/
   c. AppState::new → 初始化 5 个 service（library / collection / search / opds / rss）
   d. collection_service.ensure_position_column()
   e. library_service.close_stale_sessions() — 收尾遗留的 ended_at IS NULL
   f. TaskManager::new + init_self_ref + reconcile_on_startup — running → paused
   g. 管理 PixivSession / EhentaiSession（JSON 持久化）
   h. [macOS] WKWebView networking warmup（隐藏 1×1 webview 吸冷启动）
3. 注册 68 个 Tauri 命令
4. 加载插件（http / shell / dialog / fs / clipboard_manager / opener）
5. 运行事件循环
```

### 命令清单（68 个）

| 模块 | 数量 | 命令名 |
|---|---|---|
| book | 13 | `import_book` `delete_book` `get_book` `get_book_page` `get_book_page_count` `get_book_cover_thumb` `save_book` `save_book_page` `list_books` `open_book` `record_reading` `get_weekly_reading_ms` `list_recent_books` |
| sync | 1 | `sync_to_dir` |
| reset | 1 | `reset_app_data` |
| search | 2 | `search_books` `get_all_tags` |
| settings | 1 | `set_locale` |
| server | 4 | `start_opds_server_cmd` `stop_opds_server_cmd` `start_rss_server_cmd` `stop_rss_server_cmd` |
| pixiv | 10 | `pixiv_get_login` `pixiv_set_login` `pixiv_clear_login` `pixiv_list_bookmarks` `pixiv_list_following_feed` `pixiv_list_recommended` `pixiv_search_illusts` `pixiv_proxy_image` `pixiv_browse_status` `pixiv_open_login_window` |
| ehentai | 6 | `ehentai_open_login_window` `ehentai_get_login` `ehentai_clear_login` `ehentai_search` `ehentai_proxy_thumb` `ehentai_browse_status` |
| ahentai | 3 | `ahentai_search` `ahentai_proxy_thumb` `ahentai_browse_status` |
| nicecat | 3 | `nicecat_proxy_thumb` `nicecat_browse_status` `nicecat_fetch_api` |
| tasks | 12 | `tasks_list` `task_pause` `task_resume` `task_cancel` `task_delete` `task_retry` `tasks_clear_completed` `tasks_retry_all` `task_enqueue_ehentai_gallery` `task_enqueue_pixiv_work` `task_enqueue_ahentai_gallery` `task_enqueue_nicecat_gallery` |
| collection | 8 | `list_collections` `reorder_collections` `create_collection` `rename_collection` `delete_collection` `add_book_to_collection` `remove_book_from_collection` `get_book_collections` |
| update | 4 | `check_update` `download_update` `install_update` `quit_and_install` |

### 服务层关键常量

| 服务 | 常量 | 值 |
|---|---|---|
| aria2 | RPC endpoint | `http://localhost:6800/jsonrpc` |
| aria2 | daemon 参数 | `--split=2 --max-connection-per-server=2 --max-concurrent-downloads=16` |
| aria2 | HTTP timeout | 30s |
| proxy | 缓存 TTL | 60s |
| task_manager | 并发数 | 8（Semaphore） |
| task_manager | 任务上限 | 100（保留最新 99 + 新插 1） |
| task_manager | 最大重试 | 3 次，退避 [1, 2, 4] 秒 |
| task_manager | 日志上限 | 200 行 |
| task_manager | speed EMA | α=0.3，ticker 400ms |
| storage | PAGE_CACHE_MAX | 8 |
| OPDS 默认端口 | settings.ts | 5269 |
| RSS 默认端口 | settings.ts | 1269 |
| OPDS/RSS 监听 | server.rs | `0.0.0.0:{port}` |
| similarity | 模糊阈值 | sim ≥ 0.8，领先次优 ≥ 0.03 |
| locale | 种子词条 | 1014 条 |
| Pixiv | source_url 规范 | `https://www.pixiv.net/artworks/{work_id}` |
| EHentai | 搜索分页 | gid 游标（`?next={gid}`），25 条/页 |
| EHentai | 分类方式 | 路径段（`f_cats` 位掩码不生效） |
| NiceCat | RC4 key | `Zo1Eq4V2mr269K4doL9U4093U25acjMQ` |
| NiceCat | AUTH | `ec8be430bc634535b258b3591a414a67` |
| NiceCat | dateKey | `base64(sha256(本地午夜毫秒字符串))` |

## 4. 持久化架构

### 三层存储

```
                    SQLite (erolib.db)          权威数据层
                    ├── books (书元信息)
                    ├── tags / book_tags (标签词典 + 多对多)
                    ├── collections / collection_books (阅读列表)
                    ├── tasks (下载队列)
                    ├── reading_sessions (阅读时长)
                    ├── books_fts (全文索引，当前未启用)
                    ├── settings (KV: locale, tag_seed_fp)
                    ├── tag_translations (1014 条种子)
                    ├── tag_form_map (精确匹配物化)
                    └── tag_resolved (精确+模糊总解析)
                           │
            ┌──────────────┼──────────────┐
            ▼              ▼              ▼
     File System     IndexedDB      WKWebView Cookie
     ├── library/    └── erolib/    Store (macOS FFI)
     │   └── *.cb7       └── thumbs/
     ├── covers/
     ├── downloads/
     ├── aria2/
     ├── update/
     ├── pixiv_session.json
     └── ehentai_session.json
```

### SQLite 配置

| 参数 | 值 | 理由 |
|---|---|---|
| journal_mode | WAL | 并发下载写入不阻塞读 |
| synchronous | NORMAL | WAL 模式下安全的折中 |
| busy_timeout | 5000ms | 短暂等待避免 contention 错误 |
| foreign_keys | ON | CASCADE 删除保证引用完整性 |
| max_connections | 8 | 连接池大小 |
| after_connect | 注册 SIMILARITY UDF | 每个连接都可用模糊匹配 |

### 数据模型核心关系

```
books 1───N book_tags N───1 tags
  │                        │
  ├──N collection_books N──┤──1 collections
  │
  ├──N reading_sessions
  │
  └──0..1 tasks (book_id 回填)

tag_translations 1───N tag_form_map N───1 tag_resolved N───1 tags (by name)
```

## 5. 任务系统架构

```
前端 enqueue 命令
      │
      ▼
  TaskManager.enqueue()
      │ 保留最新 100 条
      │ INSERT task (status=pending)
      │ emit task://progress
      ▼
  run_task_worker (tokio::spawn)
      │
      ├── process_pixiv_single()    → reqwest 元数据 + aria2 8 并发下载
      ├── process_pixiv_ugoira()    → 拉 zip + 解压 jpg 帧 + delays JSON
      ├── process_ehentai()         → 串行爬 /s/ + aria2 8 并发 CDN
      ├── process_ahentai()         → reqwest 元数据 + aria2 8 并发下载
      └── process_nicecat()         → RC4 API 元数据 + aria2 8 并发下载
              │
              ▼
      完成：register_stored_book → create_cb7 → set_book_id
              │ emit task://progress + task://toast("completed")
              ▼
      失败：increment_retry → BACKOFF [1,2,4]s → 重试或 mark Failed
              │ emit task://toast("failed")
              ▼
      前端 stores/tasks.ts 监听 → 更新 UI + toast + library.refresh()
```

### TaskPayload 枚举

```rust
pub enum TaskPayload {
    EhentaiGallery { cookie, gallery_url, gid, token },
    PixivSingleWork { cookie, work_id },
    AhentaiGallery { gallery_id, title },
    NicecatGallery { comic_id, title },
}
```

### TaskStatus 状态机

```
Pending → Running → Completed
                → Failed (retry_count < 3 → Pending)
                → Cancelled
        → Paused → Running (resume)
```

## 6. 共享服务器架构（OPDS / RSS）

```
axum Router
├── /opds           → OPDS Atom root feed
├── /opds/search/:q → OPDS search feed
├── /rss            → RSS 2.0 feed
├── /covers/:id     → 封面图（jpg/jpeg/png/webp 依次尝试）
├── /pages/:id/:n   → 单页图（zip 抽页 + 魔数猜 MIME）
├── /article/:id    → HTML 图廊（<img> 逐页）
└── /download/:id   → 整本下载（CB7/CBZ/CBR/PDF）
```

**ServerState**（Clone struct）持 `db / storage / covers_path / base_url / rss_service / opds_service`，共享已配置的 service。

**幂等启停**：`ServerHandle` 各持 `watch::Sender<bool>`；已运行再启动返回错误；停止发信号触发 graceful shutdown。

## 7. 外部依赖关系

### 前端

| 包 | 版本 | 用途 |
|---|---|---|
| Vue | 3.6.0-rc.1 | 框架 |
| Pinia | 2.1.7 | 状态管理 |
| Vue Router | 4.3.0 | 路由 |
| @material/web | 2.4.1 | MD3 Web Components |
| @material/material-color-utilities | 0.3.0 | HCT 主题色生成 |
| @mdi/js | 7.4.47 | MDI 图标路径 |
| idb | 8.0.0 | IndexedDB 封装 |
| @tauri-apps/api | 2.0.0 | Tauri IPC + 事件 |

### 后端（Rust）

| 包 | 版本 | 用途 |
|---|---|---|
| tauri | 2.0 | 桌面框架 |
| sqlx | 0.7 | SQLite（WAL / 8 连接池） |
| axum | 0.7 | OPDS / RSS HTTP 服务器 |
| reqwest | 0.11 | HTTP 客户端（元数据抓取） |
| scraper | 0.18 | HTML 解析 |
| zip | 0.6 | CB7 文件读写 |
| image | 0.25 | 封面缩略图降采样 |
| quick-xml | 0.31 | ComicInfo.xml + Atom feed 解析 |
| sha2 | 0.10 | 同步文件名哈希 + NiceCat dateKey |
| base64 | 0.22 | NiceCat RC4 令牌编码 |
| rand | 0.8 | NiceCat UID 生成 |
| uuid | 1.6 | 书 / 任务 / 标签 ID |
| chrono | 0.4 | 时间处理 |

### MWC 组件清单

注册的 `@material/web` 组件（`material-web.ts` 集中 import）：

button (filled / outlined / text / filled-tonal) · iconbutton · outlined-text-field · outlined-select / select-option · switch · slider · menu / menu-item · circular-progress · linear-progress · tabs / primary-tab · dialog · icon

**不存在的 MWC 组件**（需手搓）：`md-card` · `md-top-app-bar` · `md-navigation-rail` · `md-tooltip` · `md-chip`
