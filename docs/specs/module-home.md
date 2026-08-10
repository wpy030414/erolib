# Module: 首页 (Home)

> 代码生成契约：首页仪表盘的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

首页是应用的入口页面，提供：
- 本周阅读时长统计（Hero 区）
- 库封面旋转展示墙（WallCover）
- 最近阅读书架（快捷入口）
- 书籍右键菜单（加入列表 / 查看元信息 / 保存 / 删除）

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src/views/Home.vue` | 首页视图 |
| `src/components/WallCover.vue` | 封面旋转展示墙 |
| `src/composables/useBookMenu.ts` | 右键菜单逻辑 |
| `src/components/BookCollectionPicker.vue` | 加入列表选择器 |
| `src/components/BookMetaDialog.vue` | 元信息查看 |
| `src-tauri/src/commands/book.rs` → `get_weekly_reading_ms` | 周阅读时长 |
| `src-tauri/src/commands/book.rs` → `list_recent_books` | 最近阅读列表 |

## 3. Tauri 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `get_weekly_reading_ms` | — | `number`（本周阅读毫秒数） |
| `list_recent_books` | `{ limit }` | `Book[]`（按 `last_read_at` DESC） |

## 4. Hero 区

### 本周阅读时长

- 数据来源：`get_weekly_reading_ms`
- 计算基准：**周一起点**（Monday 00:00 local），跨周一致不重不漏
- 显示格式：`X 小时 Y 分钟`

### 封面旋转墙（WallCover）

- 布局：3×7 = 21 格
- 填充逻辑：按 `book.id` 哈希稳定排序后循环填充
- 动画：奇偶列反向 40s 无缝平移
- 无书时显示占位图

## 5. 最近阅读书架

- 数据来源：`list_recent_books({ limit: 12 })`
- 排序：`last_read_at` 倒序
- 上限：12 本
- 点击卡片 → 跳转阅读器（`/reader/:id`）

### 右键菜单

每本书右键支持：
- **加入列表**：弹出 `BookCollectionPicker` 选择目标列表
- **查看元信息**：弹出 `BookMetaDialog`
- **保存到本地**：`saveBook` + `plugin-dialog` 选择目标路径
- **删除**：`deleteBook` → emit `book://deleted` → 重新拉取 12 本补位

## 6. 约束

- 删除书籍后必须重新拉取 `list_recent_books` 补位
- 新书籍立刻载入缩略图（IndexedDB 缓存）
- 删除成功/失败均有 toast
- WallCover 动画必须 CSS 实现（不用 JS 定时器）
- 周阅读时长跨周一一致（同一周内多次打开不重计）

## 7. 相关模块

- [module-reader.md](./module-reader.md) — 阅读器（点击卡片进入）
- [module-library.md](./module-library.md) — 书库（书籍管理操作）
- [module-task.md](./module-task.md) — 任务系统（下载完成后 `last_read_at` 更新）
