# AGENTS.md

## 项目概述

EroLib（工口图书馆）—— Tauri 2 + Vue 3 本地漫画库管理器，下载源支持 Pixiv 与 EHentai。UI 用 Google Material Design 3 Web Components（@material/web）手搓。应用标识符 `im.xrl.erolib`。

## 架构分层

- **前端** `src/`：Vue 3 `<script setup>` + TS + Pinia + Vue Router。MWC 组件统一在 `src/material-web.ts` 注册（别直接引 `.js`）。
- **后端** `src-tauri/`（Rust）：命令在 `src-tauri/src/commands/`，业务在 `src-tauri/src/services/`，命令注册于 `main.rs` 的 `invoke_handler`。**新增命令必须同步 `src/services/api.ts`**。
- **serde 约定**：后端 struct 透传前端时务必 `#[serde(rename_all = "camelCase")]`，否则前端读不到字段（snake_case → undefined，是高频 bug 源）。

## 状态与持久化

- **localStorage**：主题/语言、阅读器缩放模式 `erolib.reader.zoomMode`、每书阅读进度 `erolib.reader.progress.{bookId}`、Pixiv tab `erolib.pixiv.tab`、Settings tab `erolib.settings.tab`。
- **IndexedDB**：书库封面低清缩略图缓存，见 `src/services/thumb-cache.ts`（DB `erolib`，store `thumbs`，key=bookId，value=Blob）。
- **Pinia store（内存，跨视图存活到退出）**：`stores/pixiv-browse.ts`（关注/收藏 feed、封面、卡片任务状态）、`stores/library.ts`（搜索/标签/结果）。
- **滚动位置**：五个二级页面（Library/Pixiv/EHentai/Tasks/Settings）的滚动都在 `App.vue` 的 `.app-main`（`overflow:auto`），其 scrollTop 按 `route.path` 持久化到 `localStorage`（`erolib.scroll.{path}`），切回时恢复；Reader（`/reader/:id`）全屏不参与。

## 下载与任务系统

- **所有下载统一经 TaskManager**（`src-tauri/src/services/task_manager.rs`），底层用 aria2，无进程内回退。任务 payload 是枚举 `TaskPayload`：`PixivBookmarks` / `PixivUserWorks` / `PixivSingleWork{cookie,work_id}` / `EhentaiGallery`。
- 前端 `stores/tasks.ts` 全局监听 `task://progress`（更新任务列表 + 书库刷新）与 `task://toast`（终态 toast）。
- **动图（ugoira, illustType==2）**：`process_pixiv_ugoira` 拉 `ugoira_meta`（frames + originalSrc zip）→ 解压 jpg 帧 → `image` crate `GifEncoder`（speed 10、`Repeat::Infinite`、自动 NeuQuant 量化）→ 单 gif 打包 cb7，`page_count=1`。帧 resize 到最长边 1024 以保渲染性能。延时**取 API 的 frames**（zip 内只有 jpg、无 animation.json）。

## Pixiv 浏览

- 浏览式：关注 feed（`/ajax/follow_latest/illust?p=&mode=all`，**不带 user_id**，session 识别用户）+ 收藏（`/ajax/user/{id}/illusts/bookmarks?offset=&limit=`），懒加载 ~30/页（IntersectionObserver sentinel）。
- 封面防盗链：`i.pximg.net` 需 `Referer: https://www.pixiv.net/`，走后端代理 `pixiv_proxy_image`（前端 `<img>` 不能设 Referer）。
- 卡片三态：本地有→点进阅读器；下载中→遮罩 + SVG 环形进度（**别用 md-circular-progress determinate**，会卡）；未下载→标题左上红点。`task://progress` 在 **store 层**监听（跨视图存活，下载完成自动翻转卡片）。

## 书库与缩略图

- 书库封面走低清 thumb：后端 `get_book_cover_thumb`（`image` crate 降采样最长边 256px JPEG），前端先查 IndexedDB，miss 再取并缓存；原图 `get_book_cover` 给 OPDS/详情。
- 搜索框 text 匹配 title / author / **tags**；标签 chip 行并集(OR)过滤，计数随文本结果变（文本优先），上限 30，满 30 末尾加不可选 `…` chip。

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

## 开发命令

```bash
pnpm install
pnpm tauri dev                # 开发
pnpm -C . exec vite build     # 前端构建（vue-tsc 有已知兼容问题，跳过 tsc）
cd src-tauri && cargo build   # 后端构建
pnpm tauri build              # 生产包
```
