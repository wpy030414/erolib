# Module: 任务系统 (TaskManager)

> 代码生成契约：统一下载任务调度的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

`TaskManager` 是所有下载任务的唯一调度中心。它负责：
- 任务入队、暂停、继续、取消、重试、删除
- 分发四种来源的下载逻辑（Pixiv / EHentai / ASMHentai / NiceCat）
- 进度上报（`task://progress` 事件）
- 终态通知（`task://toast` 事件）
- 完成后注册书到书库（`book_id` 回填）

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/services/task_manager.rs` | 调度主逻辑（~2690 行） |
| `src-tauri/src/services/task.rs` | 数据模型 |
| `src-tauri/src/services/aria2.rs` | aria2 JSON-RPC 客户端 |
| `src-tauri/src/services/proxy.rs` | 系统 HTTP 代理检测 |
| `src-tauri/src/commands/tasks.rs` | Tauri 命令层（13 个命令） |
| `src/stores/tasks.ts` | 前端状态管理 |
| `src/views/Tasks.vue` | 任务管理页面 |
| `src-tauri/schema/schema.sql` → `tasks` 表 | 持久化 |

## 3. 数据模型

### TaskSource

```rust
enum TaskSource { Pixiv, Ehentai, Ahentai, Nicecat }
// serde: snake_case → "pixiv" | "ehentai" | "ahentai" | "nicecat"
```

### TaskStatus

```rust
enum TaskStatus { Pending, Running, Paused, Completed, Failed, Cancelled }
// serde: snake_case → "pending" | "running" | ...
```

### TaskPayload

```rust
#[serde(tag = "kind", rename_all = "camelCase")]
enum TaskPayload {
    EhentaiGallery { cookie: String, gallery_url: String, gid: String, token: String },
    PixivSingleWork { cookie: String, work_id: String },
    AhentaiGallery { gallery_id: String, title: String },
    NicecatGallery { comic_id: String, title: String },
}
// payload.source_url()：各来源规范化 books.source_url（与 process_* 写库的
// 字符串逐字节一致），重新下载按它精确匹配本地书。
```

### TaskSnapshot（前端可见）

```rust
struct TaskSnapshot {
    id: String,
    source: String,          // TaskSource.to_string()
    status: String,          // TaskStatus.to_string()
    title: String,           // "Pixiv: {title}" / "EHentai: {title}" / ...
    detail: String,
    progress_current: i64,
    progress_total: i64,
    retry_count: i32,
    max_retries: i32,        // 默认 3
    speed: i64,              // B/s，EMA α=0.3
    logs: Vec<String>,       // 带时间戳 "[HH:MM:SS.mmm] ..."，上限 200 行
    book_id: Option<String>, // 完成后回填
    total_bytes: i64,
    elapsed_ms: i64,         // 仅 running 时段累计
    created_at: String,      // RFC3339
    updated_at: String,
    completed_at: Option<String>,
}
```

## 4. 关键常量

| 常量 | 值 | 位置 |
|---|---|---|
| 并发数 | 8 | `Semaphore::new(8)` in `download_pages_concurrent` |
| 任务上限 | 100 | enqueue 时 `DELETE ... NOT IN (SELECT ... LIMIT 99)` |
| 最大重试 | 3 | `MAX_RETRIES = 3` |
| 退避 | [1, 2, 4] 秒 | `BACKOFF_SECS` |
| 日志上限 | 200 行 | `append_log` |
| speed EMA | α=0.3 | ticker 每 400ms |
| 页面失败策略 | Pixiv: `FailWholeTask`；其余: `SkipPage` | `PageErrorPolicy` |
| 最小字节数 | Pixiv: 100；其余: 200 | `min_bytes` |

## 5. Tauri 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `tasks_list` | — | `TaskItem[]` |
| `task_pause` | `{ taskId }` | `void` |
| `task_resume` | `{ taskId }` | `void` |
| `task_cancel` | `{ taskId }` | `void` |
| `task_delete` | `{ taskId }` | `void` |
| `task_retry` | `{ taskId }` | `void` |
| `task_redownload` | `{ taskId }` | `RedownloadAction`（`restarted` \| `redownloaded` \| `already_complete`） |
| `tasks_clear_completed` | — | `number`（清除数） |
| `tasks_retry_all` | — | `[number, number]`（retried, resumed） |
| `task_enqueue_pixiv_work` | `{ cookie, workId, title }` | `string`（taskId） |
| `task_enqueue_ehentai_gallery` | `{ cookie, galleryUrl, title }` | `string` |
| `task_enqueue_ahentai_gallery` | `{ galleryId, title }` | `string` |
| `task_enqueue_nicecat_gallery` | `{ comicId, title }` | `string` |

## 6. 事件

| 事件 | 载荷 | 触发时机 |
|---|---|---|
| `task://progress` | `TaskSnapshot` | 进度变化（ticker 400ms）+ 状态转换 |
| `task://toast` | `{ kind: "completed" \| "failed" \| "cancelled", title: string }` | 终态 |

## 7. 状态机

```
Pending ──run──→ Running ──ok──→ Completed
                    │                 │
                    ├──fail──→ Failed │
                    │              │  │
                    │     (retry<3)──→ Pending
                    │
                    ├──cancel──→ Cancelled
                    │
                    └──pause──→ Paused ──resume──→ Running
```

## 8. 启动恢复

`reconcile_on_startup()`：把所有遗留 `running` 状态的任务置为 `paused`（speed=0, run_started_at=NULL），让用户可以 resume。

## 8.5 完成任务重新下载（task_redownload）

仅 `Completed` 任务可用。按本地档案**实际页数**（`storage.count_pages`）对比来源
**当前远端页数**逐源判定——库内 per-source 幂等守卫只看「书行存在」，页被删的书
永远无法自愈，此命令补上这个洞：

| 判定 | 动作 | 返回 |
|---|---|---|
| 无匹配书行 | 直接 retry_task 全量重下 | `restarted` |
| 本地 = 远端 | no-op，记日志「本地完整」 | `already_complete` |
| 本地 < 远端 或文件丢失 | remove_book + retry_task 整本重下 | `redownloaded` |
| 本地 > 远端 | **拒绝执行**（疑似源站删减，覆盖会丢本地独有内容） | 报错 |

- 远端页数先行获取，任何本地变更之前完成——网络失败不动书库
- ugoira 书比较帧数（DB page_count 恒 1），其余比较列表页数
- 同源并发守卫：有其它 running 任务的 payload `source_url()` 相同则拒绝
- 执行前 `refresh_payload_cookie`：用 app 内活登录刷新 payload 里可能过期的
  cookie（Pixiv / EHentai）
- 前端 Tasks.vue 对 completed 卡片显示「重新下载」按钮，全局单飞 debounce；
  toast 按 action 区分（已完整 / 重下中 / 失败）

## 9. 前端 store 行为

- `init()` 注册三个 Tauri 事件监听
- `task://progress`：按 id 更新或 unshift
- `task://toast`：completed → `library.refresh()` + `syncIfEnabled()`
- `book://deleted`：匹配任务的 `book_id` 置 null
- 任务标题前缀：`Pixiv: ` / `EHentai: ` / `EXHentai: ` / `ASMHentai: ` / `NiceCat: `

## 10. 约束

- 所有图片下载必须走 aria2（`download_pages_concurrent`），来源客户端仅用 reqwest 抓元数据
- 任务标题必须带来源前缀
- 完成后必须 `register_stored_book`（不复制文件，直接注册已写好的 CB7）
- `book_id` 回填后前端才能一键跳阅读器
- 下载临时目录：`{app_local_data_dir}/downloads/{task_id}`
- `TaskPayload::source_url()` 必须与各 `process_*` 写库的 `books.source_url`
  逐字节一致（task.rs 注释里列了各源落点），否则重新下载匹配不到书
- 重新下载先远端后本地：远端页数拿不到时不得动任何本地数据

## 11. 相关模块

- [module-browse.md](./module-browse.md) — 浏览源（入队来源）
- [module-library.md](./module-library.md) — 书库（完成后注册书）
- [module-login.md](./module-login.md) — 登录（Pixiv/EHentai 需要 cookie）
