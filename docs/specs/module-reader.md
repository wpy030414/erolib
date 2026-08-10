# Module: 阅读器 (Reader)

> 代码生成契约：沉浸式阅读器的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

沉浸式漫画阅读器，负责：
- 全窗口沉浸阅读体验
- 键盘 / 点击 / 滑块多种翻页方式
- 动图（ugoira）原生播放
- 阅读进度记忆
- 阅读时长追踪（上报后端）
- 自定义主题提取（从书页取色）

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src/views/Reader.vue` | 沉浸式阅读器（~1074 行） |
| `src/composables/useBookMenu.ts` | 右键菜单状态管理 |
| `src-tauri/src/commands/book.rs` → `get_book_page` | 单页图片读取 |
| `src-tauri/src/commands/book.rs` → `open_book` / `record_reading` | 阅读会话 |
| `src-tauri/src/commands/book.rs` → `get_weekly_reading_ms` | 首页聚合统计 |

## 3. Tauri 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `get_book` | `{ id }` | `Book` |
| `get_book_page_count` | `{ id }` | `number` |
| `get_book_page` | `{ id, page }` | `ArrayBuffer` |
| `open_book` | `{ id }` | `number`（session_id） |
| `record_reading` | `{ id, sessionId, durationMs }` | `void` |
| `get_weekly_reading_ms` | — | `number` |
| `save_book_page` | `{ id, page, dest }` | `void` |

## 4. localStorage 键

| 键 | 数据 |
|---|---|
| `erolib.reader.zoomMode` | `'fill' \| 'contain'`（默认 contain） |
| `erolib.reader.progress.<bookId>` | 页进度（动画书不存） |
| `erolib.reader.readtime.<bookId>` | 累计阅读秒数 |

## 5. 行为约束

### 暗色模式
- mounted 时 `themeStore.setMode('dark')`
- 卸载时恢复原模式

### 缩放模式
- `contain`：`100% width + object-fit: contain`（CSS class `.reader-image--contain`）
- `fill`：`absolute + object-fit: cover`（CSS class `.reader-image--fill`）
- **不要用 inline `:style` 绑定 `object-fit`**

### 翻页
- **键盘**：ArrowRight / PageDown / 空格 → 下一页；ArrowLeft / PageUp → 上一页
- **点击**：viewport 左 1/3 上一页、右 1/3 下一页、中间 ~34% 无动作
- **预取**：`PREFETCH_SPAN = 10`，窗口外 revoke objectURL
- **自动隐藏 UI**：bar zone（顶部 64px / 底部 56px）内停住无计时器，移出 zone 启动 2s 计时隐藏

## 6. 动图 (ugoira) 播放

- 判定：`book.delays` JSON 解析成 `frameDelays`；`isAnimated = frameDelays.length > 1`
- 每帧并发 `getBookPage` → `createImageBitmap` → canvas `drawImage`
- `scheduleNextFrame`：`setTimeout(tick, Math.max(16, delay))`
- 全帧失败回退静态（`frameDelays = []`）

## 7. 阅读时长上报

```
mount → open_book(id) → session_id
每 tick（1s）:
  if document.hidden: stopReadTime()
  else:
    deltaSec = min(1, TICK_CAP_SECONDS=2)
    readTimeAccumulated += deltaSec
    saveReadTime(bookId, readTimeAccumulated)

unmount → record_reading(id, sessionId, deltaMs)
  其中 deltaMs = (readTimeAccumulated - readTimeSessionBaseline) * 1000
```

### 后端阅读会话

- `open_book`：`INSERT INTO reading_sessions (book_id, started_at, duration_ms) VALUES (?, ?, 0) RETURNING id`
  - **必须用 RETURNING**，不能用 `last_insert_rowid()`（连接池下不可靠）
  - 同时 bump `last_read_at` + `read_count`
- `record_reading`：`UPDATE reading_sessions SET ended_at = ?, duration_ms = ? WHERE id = ? AND book_id = ?`
- `close_stale_sessions`：启动时 `ended_at IS NULL` → `ended_at = started_at, duration_ms = 0`
- `get_weekly_reading_ms`：`COALESCE(SUM(duration_ms), 0) WHERE started_at >= ?`（周一起本地 0 点）

## 8. 右键菜单

- 设为主题：`sourceColorFromImage` 提取主色 → `addCustomTheme`（≤1920px 全图 + 100×100 缩略图 dataURL）
- 保存图片：`saveBookPage` + `save(plugin-dialog)`

## 9. 约束

- 进出阅读器必须强制/恢复暗色模式
- 缩放模式必须用 CSS class，不用 inline style
- 动图播放不可二次编码（保留原始 jpg 帧序列）
- 阅读时长上报仅前台（`document.hidden === false`）累计
- `TICK_CAP_SECONDS = 2` 防 WKWebView 后台冻结灌秒
- `open_book` 取 id 必须用 `INSERT … RETURNING id`

## 10. 相关模块

- [module-library.md](./module-library.md) — 书库（阅读入口）
- [module-theme.md](./module-theme.md) — 主题（暗色模式切换 + 自定义主题提取）
