# Module: 书库 (Library)

> 代码生成契约：书库管理的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

书库是 EroLib 的核心数据管理模块，负责：
- 导入外部文件（CB7 / CBZ / CBR / PDF）并统一转为 CB7
- 全文搜索（标题 / 作者 / 标签）
- 标签筛选（并集 OR，上限 30 chip）
- 阅读列表管理（创建 / 重命名 / 删除 / 加书 / 移书）
- 封面缩略图缓存协调（IndexedDB）
- 本地单向同步（书库 → 指定目录）

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src/views/Library.vue` | 书库页面 |
| `src/stores/library.ts` | 书库状态管理 |
| `src/stores/collections.ts` | 阅读列表状态 |
| `src/composables/useBookMenu.ts` | 右键菜单逻辑 |
| `src/components/CollectionDialog.vue` | 阅读列表管理抽屉 |
| `src/components/BookMetaDialog.vue` | 书籍元信息查看 |
| `src/components/BookCollectionPicker.vue` | 加入列表选择器 |
| `src-tauri/src/services/library.rs` | 后端书库服务 |
| `src-tauri/src/services/storage.rs` | CB7 文件管理 |
| `src-tauri/src/services/search.rs` | 搜索服务 |
| `src-tauri/src/services/collection_service.rs` | 阅读列表服务 |

## 3. Tauri 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `import_book` | `{ filePath }` | `Book` |
| `delete_book` | `{ id }` | `void`（emit `book://deleted`） |
| `get_book` | `{ id }` | `Book` |
| `list_books` | `{ limit?, offset? }` | `Book[]` |
| `get_book_cover_thumb` | `{ id }` | `number[]`（≤256px JPEG） |
| `save_book` | `{ id, dest }` | `void` |
| `save_book_page` | `{ id, page, dest }` | `void` |
| `search_books` | `{ query: SearchQuery }` | `SearchResult` |
| `get_all_tags` | `{ text?, collection? }` | `TagCount[]`（top 30） |
| `sync_to_dir` | `{ targetDir }` | `{ copied, skipped }` |

## 4. 搜索行为

- text 匹配 `title LIKE / author LIKE / tags.name LIKE`
- `tags`（AND，HAVING COUNT = n）
- `tags_any`（OR）
- 排序：`relevance`（默认→`created_at DESC`）/ `title` / `date` / `size`
- page_size 默认 50，clamp 1..200
- 标签 chip 行 `get_all_tags`：本地化合并同义标签，top 30，满则折叠

## 5. 文件格式

- 导入支持：CB7 / CBZ / CBR / PDF
- 下载产物：CB7（`ComicInfo.xml` + 编号图片页）
- 文件名：`{uuid}.cb7`
- 封面：`covers/{book_id}.jpg`（首张图片）

## 6. 本地同步

- 单向：书库 → 目标目录
- 文件名：`{sanitized_title}-{8位metaHash}.cb7`
- metaHash = SHA-256(source_post_id + source_url + title + page_count + file_size) 前 8 hex
- 已存在跳过；绝不删除目标文件

## 7. 阅读列表 (Collections)

| 命令 | 参数 | 返回 |
|---|---|---|
| `list_collections` | — | `Collection[]` |
| `reorder_collections` | `{ positions: [string, number][] }` | `void` |
| `create_collection` | `{ name }` | `Collection` |
| `rename_collection` | `{ id, name }` | `void` |
| `delete_collection` | `{ id }` | `void` |
| `add_book_to_collection` | `{ collectionId, bookId }` | `void` |
| `remove_book_from_collection` | `{ collectionId, bookId }` | `void` |
| `get_book_collections` | `{ bookId }` | `string[]` |

### 约束
- 上限 100 个
- 创建自动去重命名（`新的阅读列表 1/2/...`）
- 右键列表名内联重命名（input 与文字同款外观，仅光标闪烁提示）
- 重命名期间底部 `+` 变为红色垃圾桶按钮，点击弹出 `md-dialog` 确认删除

## 8. 缩略图缓存

```
loadCover(book):
  key = book.source_post_id || book.id
  blob = getThumb(key)           // IndexedDB 查询 ~0.2ms
  if !blob:
    bytes = getBookCoverThumb(id) // IPC ~3-6ms
    blob = new Blob([bytes])
    setThumb(key, blob)          // 回填 IndexedDB
  return URL.createObjectURL(blob)
```

## 9. 约束

- 删除书籍必须 emit `book://deleted` 事件（通知浏览源清除 localBookId）
- 导入时统一转 CB7，不保留原始格式
- `ComicInfo.xml` 使用 `ero:` 命名空间携带来源元信息
- 缩略图必须走低清路径（≤256px），原图仅给 OPDS/详情

## 10. 相关模块

- [module-reader.md](./module-reader.md) — 阅读器（书库条目点击进入）
- [module-browse.md](./module-browse.md) — 浏览源（下载完成后自动入书库）
- [module-tag.md](./module-tag.md) — 标签翻译（搜索时本地化合并）
- [module-opds-rss.md](./module-opds-rss.md) — 共享服务器（暴露书库内容）
