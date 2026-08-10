# DECISIONS — EroLib 设计抉择

> 记录关键设计决策的历史原因，防止架构漂移。每条决策包含：背景、选择、理由、替代方案。

---

## D-001：CB7 作为主存储格式

**背景**：漫画存档需要一种能包含元信息 + 多页图片的容器格式。

**选择**：CB7（ZIP 容器），内含 `ComicInfo.xml` + 编号图片页（`0001.jpg`, `0002.jpg`, ...）。

**理由**：
- ZIP 是最广泛支持的归档格式，所有平台都有原生支持
- ComicInfo.xml 是漫画管理的事实标准（ComicRack 等兼容）
- 自定义 `ero:` 命名空间（`xmlns:ero="https://xrl.im/erolib"`）携带来源 URL、动图延时等非标准信息
- 导入时兼容 CBZ / CBR / PDF，但统一输出 CB7

**替代方案**：
- CBR（RAR）：有专利限制，写入库不友好
- 独立文件夹 + 图片：不利于传输和管理
- SQLite BLOB 存储图片：不利于外部工具访问

---

## D-002：统一 TaskManager + aria2 下载管线

**背景**：四个来源（Pixiv / EHentai / ASMHentai / NiceCat）都需要下载图片序列。

**选择**：所有下载统一经 `TaskManager`，图片下载统一走 aria2（8 并发），来源客户端仅用 reqwest 抓取元数据。

**理由**：
- aria2 自带断点续传、代理支持、进度回调，不需要自己重新实现
- 统一管线意味着暂停/继续/取消/重试逻辑只写一次
- 来源客户端只关心「拿到图片 URL 列表」，不关心下载细节
- `Semaphore::new(8)` 控制并发，aria2 daemon 配 `--max-concurrent-downloads=16` 留两任务余量

**替代方案**：
- 每个来源独立下载逻辑：代码重复，功能不一致
- 纯 reqwest 下载：缺少断点续传和代理自动配置
- 进程内下载（不用 aria2）：无法利用 aria2 的成熟生态

---

## D-003：封面缩略图走 IndexedDB 而非文件系统

**背景**：书库网格需要快速加载数十张封面缩略图。

**选择**：后端降采样到 ≤256px JPEG → 前端缓存到 IndexedDB（库名 `erolib`，v1，objectStore `thumbs`）。

**理由**（实测数据）：
- IndexedDB 读链：~0.2ms/张（浏览器进程内，无 IPC 无序列化）
- 后端 IPC 读链：~3-6ms/张（IPC 往返 + 序列化占主要开销）
- 48 张封面加载：IndexedDB ~10ms（无缝）vs IPC ~192ms（可感知闪烁）

**替代方案**：
- 文件系统直读（通过 Tauri plugin-fs）：需要序列化 base64，更慢
- 内存缓存：不跨会话持久化
- localStorage：5MB 容量限制不够

---

## D-004：macOS WKHTTPCookieStore 原生 FFI 而非 JS eval

**背景**：Pixiv 的 PHPSESSID 是 HttpOnly cookie，JS `document.cookie` 读不到。

**选择**：`objc_msgSend` / `objc_getClass` 原始 FFI 调 `WKHTTPCookieStore.getAllCookies:`。

**理由**：
- HttpOnly cookie 是安全关键凭证，必须通过原生 API 获取
- 不依赖 `objc2` crate（避免 trait 约束和 block2 集成问题）
- 完成块用 `Block_literal` 手动构建（`_NSConcreteStackBlock` isa + `flags=0`），无需 block2

**登出清理**：按域后缀精确 `deleteCookie:completionHandler:`（不是 `deleteCookie:`），不误伤主窗口 localStorage。

**替代方案**：
- JS eval `document.cookie`：读不到 HttpOnly
- `clear_all_browsing_data`：误删其他站点 cookie
- 用 `objc2` crate：trait 约束导致编译问题

---

## D-005：ugoira 保留原始帧序列而非二次编码 GIF

**背景**：Pixiv 动图（illustType==2）是一个 zip 包含多帧 jpg + 逐帧延时。

**选择**：下载原始 jpg 帧序列存入 CB7，延时存 `books.delays`（JSON），阅读器用 canvas `drawImage` + `setTimeout` 播放。

**理由**：
- 不二次编码 → 转换瞬时（只需解压 zip）
- 无损 → 保留原始 jpg 质量和分辨率
- 逐帧延时 → 播放节奏与原站完全一致
- 阅读器按时长定时播放 → 循环无缝

**替代方案**：
- 编码为 GIF：需 color_quant 降色到 256 色，有损，转换慢，体积大
- 编码为 APNG：兼容性好些，但仍有质量和体积损失
- 编码为 WebP 动画：浏览器支持不一致

---

## D-006：NiceCat 纯 HTTP 客户端 + RC4 令牌

**背景**：`ncmm.cc` 漫画站的 API 需要动态令牌鉴权。

**选择**：Rust 端实现纯 HTTP 客户端，每请求 RC4 加密 `{uid, auth}` → base64 作为 `N-SECURITY-CERTIFICATIONS` header。

**理由**：
- 无需内嵌 WebView → 启动快、内存占用低、不增加包体积
- RC4 + 随机 48 位 uid 足够通过 API 鉴权
- 一次性令牌（复用会 403），每请求重新生成

**常量**：
- RC4 key: `Zo1Eq4V2mr269K4doL9U4093U25acjMQ`
- auth: `ec8be430bc634535b258b3591a414a67`
- dateKey: `base64(sha256(本地午夜 epoch 毫秒字符串))`

**替代方案**：
- 内嵌 WebView 爬取：启动慢，内存开销大
- 逆向 JS 客户端：维护成本高，容易被反爬

---

## D-007：OPDS / RSS 直出图片而非仅下载链接

**背景**：RSS 阅读器（Feedly、Inoreader 等）和 OPDS 客户端（Panels、Chunky）需要能直接看到图片内容。

**选择**：除 `/download/:id`（整本 CB7）外，增加 `/pages/:id/:n`（单页图）+ `/article/:id`（HTML 画廊），RSS `<content:encoded>` 内嵌全页 `<img>` 条。

**理由**：
- RSS 阅读器可直接翻看整本图片序列，不必先下载再导入
- OPDS 客户端可逐页阅读
- 复用 cb7 归档内存缓存 + `guess_image_mime` 嗅探 MIME

**替代方案**：
- 只给下载链接：用户需要下载整个 CB7 才能看
- 只给封面：不够用，用户想预览内容

---

## D-008：标签翻译走 SQL JOIN 而非应用层

**背景**：四个来源的标签语言各异（中文/日文/英文/罗马字），需要在当前界面语言下统一显示。

**选择**：`tag_translations` 表（1014 条种子）+ `tag_resolved` 物化表（精确+模糊匹配）+ SQL `COALESCE(tt.zh, tt.en, tt.romaji, name)` 在查询时翻译。

**理由**：
- 翻译发生在读时（JOIN），`tags` / `book_tags` 表永不被修改
- 精确匹配优先（`tag_form_map`），无精确匹配才走模糊（`SIMILARITY` UDF，Levenshtein ≥ 0.8）
- 物化 `tag_resolved` 避免每行模糊扫描
- `GROUP BY {disp}` 合并同义标签（多个 raw name 折叠成一个 chip）

**替代方案**：
- 应用层翻译（JS Map）：无法在 SQL `GROUP BY` 中合并同义词
- 修改 tags 表存翻译后名称：破坏数据完整性
- 每次查询实时模糊匹配：性能差

---

## D-009：`INSERT ... RETURNING id` 取阅读会话 ID

**背景**：`open_book` 需要返回新创建的 `reading_sessions` 行 ID，供后续 `record_reading` 关闭会话。

**选择**：`INSERT INTO reading_sessions (...) VALUES (...) RETURNING id`。

**理由**：
- sqlx 连接池 `max_connections=8`，`last_insert_rowid()` 是连接级的
- 两条语句可能跑在不同连接上 → `last_insert_rowid()` 返回别的连接上一次插入的 rowid（或 0）
- 后果：`record_reading` 的 `WHERE id = ?` 命不中真正那行 → 每会话 `duration_ms` 恒 0 → 首页「本周已阅读」恒 0

**已踩坑记录**：之前用 `last_insert_rowid()` 导致首页统计恒为 0，修复后正常。

---

## D-010：MWC 2.4.1 缺失组件手搓

**背景**：`@material/web` 2.4.1 缺少若干 MD3 组件。

**选择**：以下组件用 MD3 design tokens 手搓：

| 缺失组件 | 替代方案 |
|---|---|
| `md-card` | `.md3-card` + token CSS |
| `md-top-app-bar` | `.view-header` flex 布局 |
| `md-navigation-rail` | `.nav-rail`（84px 宽左侧栏） |
| `md-tooltip` | 浏览器原生 `title` 属性 |
| `md-chip` | `.cat-chip` / `.tag-chip` button |
| 环形进度（determinate） | 手搓 SVG（MWC determinate 频繁更新会卡） |

**理由**：不依赖不存在的组件，保持 MD3 视觉一致性。

---

## D-011：全局 `user-select: none`

**背景**：桌面应用的文本选择行为与 Web 不同——用户不期望拖选 UI 元素。

**选择**：`md3.css` 全局 `user-select: none` + `*:focus-visible { outline: none }`。

**理由**：
- 拖选会产生不可预期的蓝色高亮，破坏 MD3 视觉
- MWC 组件自带 focus outline 与全局主题不一致
- 输入框 / textarea / `[contenteditable]` 例外保持选中 + 光标

**注意**：不要在组件里单独加 `user-select` 或 `outline`——全局规则已覆盖。

---

## D-012：前端默认端口 5269 / 1269（非 8080 / 8081）

**背景**：OPDS / RSS 服务器需要默认端口。

**选择**：OPDS `5269`、RSS `1269`（前端 `settings.ts` 默认值）。

**理由**：
- 避开常用端口（8080 常被开发服务器占用）
- 5269 = "E-RO"（工口）谐音
- 1269 = 易记
- 后端 `opds.rs` / `rss.rs` 的 `base_url` 默认只是 `localhost:8080/8081` 回退，实际启动时由前端传入端口覆盖

---

## D-013：schema 幂等执行而非 migrate 版本号

**背景**：`sqlx::migrate!` 宏在当前工具链的 release profile 下无法编译。

**选择**：`schema.sql` 全量 `CREATE IF NOT EXISTS`，启动时直接执行；不做 `PRAGMA user_version` 版本跟踪。

**理由**：
- 所有 DDL 幂等，重复执行是 no-op
- 避免了 `sqlx-macros` 的编译问题
- 新增列走启动时 `ensure_*` 方法（如 `ensure_position_column`）
- `tag_translations.sql` 的 `INSERT OR IGNORE` 按固定 id 幂等

**代价**：无法检测旧版本 schema 并做破坏性迁移；新增列/表可以，改列名/类型需要手动处理。

---

## D-014：自动更新走 GitHub releases Atom feed + aria2 下载

**背景**：用户需要方便地获取新版本，而非手动去 GitHub 下载。

**选择**：启动时静默请求 GitHub releases Atom feed（`/repos/{owner}/{repo}/releases.atom`），解析最新版本号与当前版本（`tauri.conf.json` `version`）比对；有更新时弹 `UpdateDialog`，下载走 aria2（复用代理检测），安装时替换 .app（macOS）或启动 .msi（Windows）。

**理由**：
- Atom feed 无需 GitHub API token（公开仓库匿名可访问，不受 rate limit 60/h 约束）
- 复用 aria2 下载管线 = 自动代理 + 断点续传 + 进度回调，不写新下载逻辑
- `UpdateProgress` 事件（`update://progress`）前端实时展示进度 / 速度
- 版本号用日期格式 `YY.M.D+HHmm`（`26.8.14+0015`），语义化且单调递增

**替代方案**：
- GitHub REST API `/releases/latest`：需要 token 或受 rate limit 约束
- Tauri 官方 updater plugin：对自定义逻辑（如 Atom feed 解析）支持不足
- 手动下载（不自动更新）：用户需要自己去 GitHub 找新版本

---

## D-015：Vite 8 + rolldown + OXC minify（包体积优化）

**背景**：随着功能增长（4 个下载源 + 阅读器 + 主题引擎），前端包体积开始影响启动速度。

**选择**：升级到 Vite 8（rolldown 替代 esbuild 做 bundle，OXC 替代 terser 做 minify）；`vite.config.ts` 中 `manualChunks` 拆分 vendor 包（material / vue / tauri / mdi / idb / color），路由级 lazy-load（`() => import('@/views/...')`）。

**理由**：
- rolldown 是 Rust 实现的 rollup 替代，bundle 速度更快
- OXC minify 比 terser 快 10× 且压缩率接近
- `manualChunks` 让第三方库独立 chunk，浏览器可长期缓存
- 路由 lazy-load 使初始 bundle 仅含 router + Home，其他页面首次导航才加载

**替代方案**：
- 继续用 Vite 5 + esbuild + terser：构建速度慢，包体积略大
- 不用 manualChunks：所有 vendor 打到一个 chunk，每次更新都要重新下载全部

---

## D-016：Vue Router 路由级 lazy-load

**背景**：9 个页面视图（Home / Library / Reader / Pixiv / EHentai / AHentai / NiceCat / Tasks / Settings）全部打进初始 bundle 会导致首屏白屏时间过长。

**选择**：`router/index.ts` 中所有路由组件使用 `() => import('@/views/Xxx.vue')` 动态导入，`createWebHashHistory()` 保持 hash mode。

**理由**：
- 初始 bundle 仅含 router + Home + AppShell，~150KB gzip
- 其他视图首次导航时才加载（Library ~80KB, Reader ~120KB, Settings ~60KB 等）
- hash mode 兼容 Tauri `tauri://localhost` 协议（不需要服务端 fallback）

**替代方案**：
- history mode：需要 Tauri 配置 fallback，增加复杂度
- 不打 lazy-load：首屏 bundle 过大（>500KB），白屏时间长

---

## D-017：MWC 组件按需注册（material-web.ts + 各 view 内 lazy import）

**背景**：`@material/web` 2.4.1 包含 50+ 组件，但大多数页面只用其中几个。

**选择**：全局核心组件（`filled-button` / `circular-progress` / `icon`）在 `material-web.ts` 注册；视图特定组件（`tabs` / `slider` / `select` / `dialog` 等）在各 view 的 `<script setup>` 中按需 `import`。

**理由**：
- 按需注册 = tree-shaking，未用的组件不进 bundle
- 全局核心组件避免每个 view 重复 import
- `vite.config.ts` 的 `optimizeDeps.include: ['@/material-web.ts']` 确保核心组件预打包

**替代方案**：
- 全部在 `material-web.ts` 注册：bundle 体积大（多了 ~30 个未用组件）
- 全部在各 view 注册：核心组件重复 import，Vite 仍需 tree-shake

---

## D-018：卡片标题 marquee 动画（hover 触发 + 溢出检测）

**背景**：漫画标题经常很长（日文轻小说式标题），卡片网格中会被 truncate。

**选择**：`main.ts` 全局 `mouseover` 监听 `.md3-card__title`，首次 hover 时测量 `inner.scrollWidth - title.clientWidth`，溢出才启用 CSS 动画（`@keyframes marquee` 从 0 滚到 `--title-scroll`，速度 2rem/s，末端停顿 ~3s）。

**理由**：
- 不溢出不动画 = 短标题无干扰
- 首次 hover 才测量 = 避免初始渲染时大量 layout 计算
- 全局监听而非组件内 = 无需改 SourceCard / Library 等组件
- `data-marquee-ready="1"` 标记避免重复测量

**替代方案**：
- CSS `text-overflow: ellipsis`：截断后看不到完整标题
- 始终滚动：短标题也滚动，视觉干扰
- tooltip：hover 时弹 tooltip，但不如 marquee 直观

