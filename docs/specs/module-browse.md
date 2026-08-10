# Module: 浏览源 (BrowseFeed)

> 代码生成契约：四个浏览源（Pixiv / EHentai / ASMHentai / NiceCat）的通用基础设施。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

`useBrowseFeed` 是一个 Vue composable，封装了浏览源的通用机制：
- 分页缓冲（跨源统一 48 条/页）
- 封面加载（IndexedDB 缓存 + 后端代理 + 并发控制）
- 卡片三态（本地已有 / 下载中 / 未下载）
- 事件监听（`task://progress` + `book://deleted`）

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src/composables/useBrowseFeed.ts` | 通用 composable |
| `src/composables/useInfiniteSentinel.ts` | IntersectionObserver 哨兵 |
| `src/composables/useBookMenu.ts` | 书库卡片右键菜单逻辑 |
| `src/composables/useDebouncedModel.ts` | 防抖 ref（搜索输入） |
| `src/stores/pixiv-browse.ts` | Pixiv 4 feed |
| `src/stores/ehentai-browse.ts` | EHentai 1 feed |
| `src/stores/ahentai-browse.ts` | ASMHentai 1 feed |
| `src/stores/nicecat-browse.ts` | NiceCat 1 feed + 首页 |
| `src/components/SourceCard.vue` | 统一卡片组件 |
| `src/components/FeedList.vue` | 网格脚手架 |
| `src/services/thumb-cache.ts` | IndexedDB 封面缓存 |

## 3. 接口

### UseBrowseFeedOptions

```typescript
interface UseBrowseFeedOptions<TItem, TKey, TStatus, TCursor> {
  keyOf: (item: TItem) => TKey;
  statusKeyOf: (status: TStatus) => TKey;
  coverKeyOf?: (item: TItem) => string;       // 默认 = keyOf
  coverUrlOf: (item: TItem) => string | null;
  fetchStatus: (keys: TKey[]) => Promise<TStatus[]>;
  fetchPage: (cursor: TCursor) => Promise<{ items: TItem[]; nextCursor: TCursor; end: boolean }>;
  proxyCover: (url: string) => Promise<number[]>;
  initialCursor: TCursor;
  shared?: BrowseFeedShared<TStatus>;         // Pixiv 4 feed 共享
  listen?: boolean;                           // 默认 true
}
```

### BrowseFeedShared

```typescript
interface BrowseFeedShared<TStatus> {
  coverMap: Record<string, string | null>;
  statusMap: Record<string, TStatus>;
  coverLoading: Set<string>;
}
```

## 4. 关键常量

| 常量 | 值 | 说明 |
|---|---|---|
| `BROWSE_PAGE_SIZE` | 48 | 网格固定页大小，跨源缓冲 |
| `COVER_MAX_CONCURRENT` | 6 | 封面代理并发上限 |
| `TERMINAL` | `['completed', 'failed', 'cancelled']` | 任务终态 |

## 5. 分页机制

```
loadMore():
  while buffer.length < 48 && !sourceEnded:
    res = fetchPage(cursor)
    buffer.push(...res.items)
    cursor = res.nextCursor
    if res.end: sourceEnded = true
  page = buffer.splice(0, 48)
  refreshStatus(page.map(keyOf))
  feed.items.push(...page)
```

## 6. 封面加载

```
loadCover(item):
  key = coverKeyOf(item)
  if key in coverMap || key in coverLoading: return
  coverLoading.add(key)
  coverGateEnter() // 6 并发门闩
  blob = getThumb(key) // IndexedDB
  if !blob:
    bytes = proxyCover(coverUrlOf(item))
    blob = new Blob([bytes], { type: 'image/jpeg' })
    setThumb(key, blob)
  coverMap[key] = URL.createObjectURL(blob)
  coverLoading.delete(key)
  coverGateLeave()
```

## 7. 事件监听（listen=true 时）

- `task://progress`：按 taskId patch `taskStatus / progressCurrent / progressTotal`；终态后重新 `refreshStatus([key])`
- `book://deleted`：匹配 `localBookId` 的 status 行清除 `localBookId`

## 8. 四个来源的配置

| 来源 | keyOf | coverKeyOf | 分页方式 | 每页大小 | 特殊行为 |
|---|---|---|---|---|---|
| Pixiv | `work.id` | `work.id` | recommend: 一次性；following: page；bookmark: offset/total；search: page | ~30/~60 | 4 feed 共享 maps |
| EHentai | `galleryUrlOf(item)` | `item.gid` | gid 游标（`?next={gid}`） | 25 | EX 切换重键所有 URL |
| ASMHentai | `item.id` | `item.id` | page 数字 | 20 | 无登录 |
| NiceCat | `item.uid` | `item.uid` | searchId 游标（`''`=第 1 页） | 60 | 首页横向滚动分区 |

## 9. 卡片三态（CardStatus）

```typescript
interface CardStatus {
  localBookId?: string;    // 本地有 → 点进阅读器
  taskId?: string;         // 下载中 → 遮罩 + 进度环
  taskStatus?: string;
  progressCurrent: number;
  progressTotal: number;
}
```

`SourceCard` 渲染逻辑：
- `localBookId` 存在 → 已下载态
- `taskId` 存在且 status ∈ `['pending', 'running', 'paused']` → 忙态遮罩
- 否则 → 未下载态（红点提示，仅当 `status` prop 有值时显示）

## 10. 约束

- 封面必须先查 IndexedDB，miss 才调后端代理
- 封面代理并发上限 6
- 网格每页固定 48 条，不随源页大小变化
- `listen: true` 只能有一个实例 armed（Pixiv 仅 recommend 实例 armed）
- 封面键用稳定标识（Pixiv: workId；EHentai: gid 而非 URL）

## 11. 相关模块

- [module-task.md](./module-task.md) — 任务系统（卡片状态来源）
- [module-library.md](./module-library.md) — 书库（`book://deleted` 事件来源）
- [module-login.md](./module-login.md) — 登录（Pixiv/EHentai 需要 cookie）
