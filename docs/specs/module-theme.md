# Module: 主题与国际化 (Theme & i18n)

> 代码生成契约：Material Design 3 动态主题 + 三语界面的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

- Material Design 3 动态主题（内置种子色 + 自定义主题 + 明暗模式）
- 三语界面切换（中文 / English / 日本語）
- 全局 design tokens 管理

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src/stores/theme.ts` | Pinia 主题状态管理 |
| `src/services/md3-theme.ts` | MD3 主题引擎（HCT 色彩空间） |
| `src/styles/tokens.css` | MD3 design tokens（`--md-sys-color-*`） |
| `src/styles/md3.css` | 全局基础样式 + `user-select: none` + 手搓组件 |
| `src/i18n/index.ts` | i18n 初始化 + `t()` 函数 + `setLocale()` |
| `src/i18n/zh.ts` | 中文字典（154 键） |
| `src/i18n/en.ts` | 英文字典（154 键） |
| `src/i18n/ja.ts` | 日文字典（154 键） |
| `src/material-web.ts` | MWC 组件全局注册（核心组件） |
| `src/main.ts` | 启动时主题初始化（防白闪） |

## 3. 主题系统

### 内置种子色

| 名称 | 色值 | 说明 |
|---|---|---|
| `pink` | `#ab2a72` | 默认 |
| `violet` | `#8320c0` | — |
| `blue` | `#204cd0` | — |
| `teal` | `#00605c` | — |

### 自定义主题

- 来源：阅读器右键菜单「设为主题」→ `sourceColorFromImage` 提取主色
- 上限：3 个
- 存储：`localStorage` → `erolib.customThemes`（JSON `Record<string, { seedColorHex, name }>`）
- key 格式：`custom:<uuid>`

### 明暗模式

- 存储：`localStorage` → `erolib.theme`（`'light' | 'dark'`）
- 阅读器进出强制暗黑，退出恢复

### localStorage 键

| 键 | 数据 |
|---|---|
| `erolib.seed` | `pink\|violet\|blue\|teal\|custom:<uuid>` |
| `erolib.theme` | `light\|dark` |
| `erolib.customThemes` | JSON `Record<string, CustomTheme>` |

### 启动序列（main.ts）

```
1. readSavedTheme() → { seed, mode }
2. if seed.startsWith('custom:'):
     从 localStorage 取 hex → applyArgbTheme(argb, isDark)
   else:
     applyMd3Theme(seed, mode)
3. 挂载 app
4. themeStore.setSeed(seed) 同步 Pinia
5. themeStore.setMode(mode) 同步 Pinia
```

### 主题引擎（md3-theme.ts）

- `applyMd3Theme(seed, mode)`：内置种子 → `argbFromHex` → `themeFromSourceColor` → 写 CSS vars
- `applyArgbTheme(argb, isDark)`：自定义种子 → 同上
- 输出：`--md-sys-color-*`（primary / on-primary / surface / on-surface 等 ~30 个 token）

## 4. 国际化系统

### Locale 类型

```typescript
type Locale = 'zh' | 'en' | 'ja';
```

### 字典

- 每个字典是 `Record<string, string>`，154 个键
- 键格式：`namespace.key`（如 `nav.library`、`lib.search.placeholder`、`reader.zoom.fill`）
- 变量插值：`{varName}` → `t('key', { varName: value })`

### 初始 locale 检测

```
1. localStorage.getItem('erolib.locale') → 有效则用
2. navigator.language:
   - startsWith('ja') → 'ja'
   - startsWith('en') → 'en'
   - else → 'zh'
```

### 切换流程

```
setLocale(l):
  1. locale.value = l
  2. applyWindowTitle()
  3. localStorage.setItem('erolib.locale', l)
  4. api.setLocale(l) fire-and-forget（后端标签翻译用）
  5. onLocaleChange callbacks（如 libraryStore.refresh()）
```

### 翻译函数

```typescript
function t(key: string, vars?: Record<string, string | number>): string
```

- 优先当前 locale 字典
- 回退 zh 字典
- 最终回退 key 本身

### localStorage 键

| 键 | 数据 |
|---|---|
| `erolib.locale` | `'zh' \| 'en' \| 'ja'` |

## 5. 全局样式约束

### `md3.css` 全局规则

```css
* { user-select: none; }
*:focus-visible { outline: none; }
input, textarea, [contenteditable] { user-select: text; }
```

- **不要在组件里单独加 `user-select` 或 `outline`**——全局规则已覆盖
- 输入框 / textarea / `[contenteditable]` 例外保持选中 + 光标

### MWC 组件注册

`material-web.ts` 全局注册核心组件：
- `filled-button` / `circular-progress` / `icon`

视图特定组件（`tabs` / `slider` / `select` / `dialog` 等）在各 view 的 `<script setup>` 中按需 import。

## 6. 约束

- 自定义主题上限 3 个
- 内置种子色不可删除
- 明暗模式切换必须保留用户选择（不跟随系统自动切换）
- 阅读器强制暗色模式不影响全局持久化
- i18n 字典键必须三语对齐（缺失键回退 zh）
- `t()` 函数不可用于 SQL 查询（SQL 翻译走 `display_expr`）

## 7. 相关模块

- [module-reader.md](./module-reader.md) — 阅读器（强制暗色 + 自定义主题提取）
- [module-tag.md](./module-tag.md) — 标签翻译（后端 locale 同步）
- [module-library.md](./module-library.md) — 书库（locale 切换后刷新标签 chip）
