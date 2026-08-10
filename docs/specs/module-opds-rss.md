# Module: 共享服务器 (OPDS / RSS)

> 代码生成契约：内置 OPDS / RSS HTTP 服务器的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

内置 HTTP 服务器把书库共享给同一局域网内的阅读器（Panels、Chunky 等）和 RSS 订阅器。

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/commands/server.rs` | Tauri 命令 + axum Router + handler |
| `src-tauri/src/services/opds.rs` | OPDS Atom feed 生成 |
| `src-tauri/src/services/rss.rs` | RSS 2.0 feed 生成 |
| `src-tauri/src/services/feed.rs` | 共享元信息渲染（`book_metadata_blurb`） |
| `src/stores/settings.ts` | 前端端口管理 + 启停 |

## 3. Tauri 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `start_opds_server_cmd` | `{ port }` | `string`（base_url） |
| `stop_opds_server_cmd` | — | `void` |
| `start_rss_server_cmd` | `{ port }` | `string`（base_url） |
| `stop_rss_server_cmd` | — | `void` |

## 4. 默认端口

| 服务 | 默认端口 | 持久化键 |
|---|---|---|
| OPDS | 5269 | `erolib.opdsPort` |
| RSS | 1269 | `erolib.rssPort` |

## 5. 路由

### OPDS

| 路径 | 说明 | Content-Type |
|---|---|---|
| `/opds` | Root feed（Atom） | `application/atom+xml;profile=opds-catalog` |
| `/opds/search/:query` | Search feed | 同上 |
| `/covers/:id` | 封面图 | `image/jpeg` / `image/png` / `image/webp` |
| `/pages/:id/:n` | 单页图（0 基） | 按魔数猜 MIME |
| `/article/:id` | HTML 图廊 | `text/html` |
| `/download/:id` | 整本下载 | `application/x-cb7` / `application/x-cbz` / ... |

### RSS

| 路径 | 说明 | Content-Type |
|---|---|---|
| `/rss` | RSS 2.0 feed | `application/rss+xml; charset=utf-8` |
| `/covers/:id` | 封面图 | 同上 |
| `/pages/:id/:n` | 单页图 | 同上 |
| `/article/:id` | HTML 图廊 | `text/html` |
| `/download/:id` | 整本下载 | 同上 |

## 6. 网络配置

- 监听：`0.0.0.0:{port}`（LAN-wide）
- base_url：`http://{local_lan_ip()}:{port}`
- `local_lan_ip()`：UDP connect `8.8.8.8:80` 探测出口 IP，失败回退 `127.0.0.1`
- **无鉴权**

## 7. 幂等启停

- `ServerHandle` 各持 `Mutex<Option<watch::Sender<bool>>>`
- 已运行再启动 → 返回 `"OPDS/RSS server is already running"`
- 停止发 `tx.send(true)` 触发 graceful shutdown

## 8. Feed 内容

### OPDS Atom

- feed 标题：`EroLib`
- feed id：`urn:uuid:9f3c7a2b-4e1d-4a6b-8c72-3e9f0d1a5b6c`
- entry：`<link rel="http://opds-spec.org/acquisition" href="{base}/download/{id}" type="application/x-cb7"/>`
- 缩略图：`{base}/covers/{id}`
- `<summary type="html">` = `book_metadata_blurb`（CDATA）
- 排序：`created_at DESC`
- 标签本地化：`locale::display_expr` + `tag_join`

### RSS 2.0

- channel 标题：`EroLib`
- description：`EroLib 本地书库 RSS 订阅`
- language：`zh-cn`
- 每 item：
  - `<link>` → `{base}/article/{id}`（HTML 图廊）
  - `<guid>` = `urn:uuid:{id}`
  - `<content:encoded>` 内嵌全页 `<img src="{base}/pages/{id}/{n}">`
  - `<enclosure url="{base}/download/{id}" length={file_size} type={mime}>`
  - `<media:thumbnail>` = `{base}/covers/{id}`

### book_metadata_blurb（feed.rs 共享）

HTML `<br>` 多行：
- 作者（含 author_id）
- `{page_count} 页 · {FORMAT} · {size}`
- 标签列表
- 来源 + 作品 ID
- 链接（`<a>`）
- 原始文件名
- 发布日期
- 收录日期
- 阅读次数 + 最近阅读时间

## 9. ServerState

```rust
struct ServerState {
    db: Arc<Database>,
    storage: Arc<StorageService>,
    storage_path: PathBuf,
    covers_path: PathBuf,
    base_url: Arc<String>,
    rss_service: Arc<RssService>,
    opds_service: Arc<OpdsService>,
}
```

## 10. 前端行为

- `App.vue` `onMounted` → `settingsStore.autoStartAll()`（并行 startOpds + startRss）
- Settings → Sharing tab 管理端口与启停
- 端口修改自动持久化到 localStorage

## 11. 约束

- 启动必须幂等（重复启动不报错）
- 停止必须 graceful（不丢请求）
- feed 标题统一 `EroLib`（非 "EroLib Library"）
- `/pages/:id/:n` 复用 CB7 归档内存缓存（`PAGE_CACHE_MAX=8`）
- `/download/:id` 的 `Content-Disposition` 带 sanitized 文件名

## 12. 相关模块

- [module-library.md](./module-library.md) — 书库（共享内容来源）
- [module-tag.md](./module-tag.md) — 标签翻译（feed 中标签本地化）
