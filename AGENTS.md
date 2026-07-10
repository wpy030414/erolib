# AGENTS.md

## 项目概述

EroLib 是一个 Tauri 2 + Vue 3 的本地漫画库管理器，UI 使用 Google 官方 Material Design 3 Web Components（@material/web）构建。

## 关键约定

### 前端

- 使用 Vue 3 `<script setup>` + TypeScript。
- 状态管理使用 Pinia；全局持久化配置（主题、语言、阅读器设置）写入 `localStorage`。
- 多语言通过 `src/i18n/index.ts` 实现，三语字典为 `zh.ts`、`en.ts`、`ja.ts`。
- MWC 组件统一在 `src/material-web.ts` 注册，不要直接引用 `.js` 文件（避免路径错误）。
- MWC 事件绑定：
  - `md-outlined-text-field`：使用 `:value` + `@input`。
  - `md-switch`、`md-tabs`、`md-outlined-select`、`md-slider`：使用 `ref` + `addEventListener('change' | 'input')`，并在 `onBeforeUnmount`/`onUnmounted` 清理。
- 图标使用 `@mdi/js` 的 SVG path，通过手搓 `<svg>` 或 `<MdiIcon>` 组件插入；避免把 Vue 组件作为 MWC 自定义元素的 slot 内容（升级时机可能导致不识别）。

### 主题

- `src/services/md3-theme.ts` 根据 seed + light/dark 生成 `--md-sys-color-*` CSS 变量。
- 修改主题后调用 `applyMd3Theme(seed, mode)` 即可全局生效。
- 阅读器强制暗黑模式：进入时保存原模式，退出时恢复。

### 后端

- Rust 命令注册在 `src-tauri/src/main.rs` 的 `invoke_handler` 中。
- 新增命令需同步更新 `src/services/api.ts`。
- 应用标识符为 `im.xrl.erolib`。

### 阅读器

- 一级页面，无侧边栏。
- 上下工具栏悬浮，光标离开后 2 秒自动隐藏。
- 默认显示模式为「展示完全内容」（contain），持久化到 `localStorage`。
- 每本书的阅读进度持久化：`erolib.reader.progress.${bookId}`。

### Pixiv

- 登录状态在启动时恢复。
- 关注列表缓存 24 小时，点击刷新才强制拉取。
- 关注下载只支持下拉选择已关注 P 主，不再手动填写用户 ID。

## 常见陷阱

- MWC 2.4.1 没有 `md-card`、`md-top-app-bar`、`md-navigation-rail`、`md-tooltip`，这些布局需用 token 手搓。
- `md-icon-button` 的 import 路径是 `@material/web/iconbutton/icon-button.js`，不是 `button/icon-button.js`。
- Vue 模板不要对 `md-slider` 使用 `:value` 单向绑定，否则拖动会被 Vue 写回覆盖。

## 开发命令

```bash
pnpm install
pnpm tauri dev
pnpm exec vue-tsc --noEmit
cd src-tauri && cargo check
pnpm tauri build
```
