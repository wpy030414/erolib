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
| `src/components/BookExportDialog.vue` | 导出格式选择（cb7/epub/pdf） |
| `src/components/BookCollectionPicker.vue` | 加入列表选择器 |
| `src-tauri/src/services/library.rs` | 后端书库服务 |
| `src-tauri/src/services/storage.rs` | CB7 文件管理 |
| `src-tauri/src/services/export.rs` | 导出格式封装（cb7/epub/pdf） |
| `src-tauri/src/services/import.rs` | EPUB/PDF 导入解析 |
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
| `save_book` | `{ id, dest, format? }` | `void`（format: cb7/epub/pdf，默认 cb7） |
| `save_book_page` | `{ id, page, dest }` | `void` |
| `delete_page` | `{ id, page }` | `number`（重打包后的新页数） |
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

- 导入支持：CB7 / CBZ / CBR / EPUB / PDF
- 下载产物：CB7（`ComicInfo.xml` + 编号图片页）
- 文件名：`{uuid}.cb7`
- 封面：`covers/{book_id}.jpg`（首张图片）

### 导出（save_book 自选格式）

| 格式 | 行为 | 元信息载体 |
|---|---|---|
| `cb7` | 原样复制库内文件 | `ComicInfo.xml`（ero: 命名空间） |
| `epub` | 图片原样内嵌 + 每图一 XHTML 页 | OPF `<dc:*>` + `<meta property="ero:...">` |
| `pdf` | JPEG 走 DCTDecode 流原样嵌入；PNG/WebP 解码为 RGB 裸像素；每图一页（页尺寸 = 图像像素 @96dpi） | Info 字典（Title/Author/Subject/Keywords）+ Catalog `/Metadata` 的 JSON 流 |

- 导出命令签名：`save_book { id, dest, format? }`，format 省略时为 `cb7`
- 前端经 `BookExportDialog.vue` 选择格式后调用
- 三种格式的元信息均可被导入读回（round-trip 有单测覆盖：
  `services::export::tests::{epub,pdf}_round_trips_metadata`）

### 导入读回

- CB7/CBZ：读 `ComicInfo.xml`
- EPUB：解析 OPF spine 顺序 + `<img src>` 取图；`dc:title/creator/description/subject` + `ero:` meta 还原元信息
- PDF：遍历每页 Resources/XObject 首个图像流提取页面（DCTDecode 直接取原始 JPEG 字节；无 filter 裸像素按 Width/Height 组装重编码 JPEG）；Catalog `/Metadata` 的 erolib JSON 流还原元信息，无则退回 Info 字典
- EPUB/PDF 导入后统一重新打包为库内 CB7（reader 仅支持 zip 图片序列）

### 单页删除（delete_page）

- 阅读器右键菜单「删除本页」（动画书隐藏）→ `delete_page { id, page }`
- 后端两段式：`storage.rewrite_without_page`（spawn_blocking，zip 重打包）+
  `library.finalize_page_deletion`（DB page_count 更新 + 删首页时重提封面）
- zip 无删除操作：全部剩余页写入 `.cb7.tmp` 后 rename 覆盖，崩溃不损原件
- 删到最后一页拒绝（至少保留一页）；删除第 0 页后前端 `deleteThumb` 清缓存、
  后端重提封面
- 前端原子换态：替换前先取好下一页 blob，整体替换 `blobs` 对象保证无骨架闪烁；
  删末页 clamp 时旧 blob 直接复用

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
- 导入时统一转 CB7，不保留原始格式（EPUB/PDF 解析后重新打包）
- `ComicInfo.xml` 使用 `ero:` 命名空间携带来源元信息
- 缩略图必须走低清路径（≤256px），原图仅给 OPDS/详情
- PDF 图片流不能用 lopdf 的 `decompressed_content()`（对 Subtype=Image 会报错），
  必须直接读 `stream.content` 并按 Filter 自行解码
- printpdf 的 glob 导出（`use printpdf::*`）会遮蔽外部 `image` crate，禁止使用
- 单页删除必须走临时文件 + rename，禁止原地改写；重打包前后各驱逐一次归档缓存

## 10. 相关模块

- [module-reader.md](./module-reader.md) — 阅读器（书库条目点击进入）
- [module-browse.md](./module-browse.md) — 浏览源（下载完成后自动入书库）
- [module-tag.md](./module-tag.md) — 标签翻译（搜索时本地化合并）
- [module-opds-rss.md](./module-opds-rss.md) — 共享服务器（暴露书库内容）
