# EroLib

工口图书馆 —— 基于 Tauri 2 + Vue 3 的本地漫画库管理器，支持 Pixiv 与 EHentai 下载。

## 技术栈

- **桌面框架**：Tauri 2
- **前端**：Vue 3.5 + Vue Router + Pinia + TypeScript + Vite 8
- **UI 组件**：Google Material Design 3 Web Components（[@material/web](https://github.com/material-components/material-web)）
- **主题**：@material/material-color-utilities 动态生成 MD3 token，支持动态主题色与暗黑模式
- **图标**：@mdi/js SVG 图标

## 功能

- 本地书库：导入 CB7/CBZ/CBR/PDF，浏览封面网格，搜索与删除
- 阅读器：沉浸式全窗口阅读，支持「充满屏幕 / 展示完全内容」两种模式，进度滑块与键盘翻页，自动保存阅读进度
- Pixiv：登录后下载收藏作品，或从关注列表选择 P 主下载其作品（关注列表缓存 24h）
- EHentai：登录后按画廊 URL 下载
- OPDS / RSS 服务器：在设置中启用，供外部阅读器访问
- 多语言：中文 / English / 日本语

## 快速开始

```bash
# 安装依赖
pnpm install

# 开发模式
pnpm tauri dev

# 构建生产包
pnpm tauri build
```

## 项目结构

```
erolib/
├── src/                 # 前端 Vue 源码
│   ├── components/      # 共享组件
│   ├── i18n/            # 三语字典
│   ├── material-web.ts  # MWC 组件注册
│   ├── services/        # API、主题引擎
│   ├── stores/          # Pinia stores
│   ├── styles/          # MD3 token 与工具类
│   ├── views/           # 页面（Library / Reader / Pixiv / EHentai / Settings）
│   └── ...
├── src-tauri/           # Rust 后端
├── docs/                # 文档
└── peacock.png          # 应用图标源文件
```

## 开发者提示

- MWC 组件标签以 `md-` 开头，Vue 中已通过 `template.compilerOptions.isCustomElement` 处理。
- `md-select`、`md-tabs`、`md-switch`、`md-slider` 的 `change` 事件不是 composed，需通过 `ref` + `addEventListener` 绑定，并在卸载时清理。
- 阅读器强制暗黑模式，退出后恢复用户设置。

## License

MIT
