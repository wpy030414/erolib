# Module: 自动更新 (Auto Update)

> 代码生成契约：应用自动更新的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

启动时静默检查 GitHub releases，有新版本时通知用户，下载并安装更新。

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src/stores/update.ts` | 更新状态管理 |
| `src/components/UpdateDialog.vue` | 更新提示对话框 |
| `src-tauri/src/commands/update.rs` | 检查 / 下载 / 安装命令 |
| `src-tauri/src/services/update.rs`（如存在） | Atom feed 解析 + 版本比对 |

## 3. Tauri 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `check_update` | — | `UpdateInfo` |
| `download_update` | `{ url, name }` | `string`（下载路径） |
| `install_update` | `{ path }` | `void` |
| `quit_and_install` | `{ path }` | `void` |

## 4. 数据模型

### UpdateInfo

```typescript
interface UpdateInfo {
  current: string;     // 当前版本号（tauri.conf.json version）
  latest: string;      // 最新版本号
  hasUpdate: boolean;  // current < latest
  notes: string;       // release notes (HTML)
  asset: UpdateAsset | null;
}

interface UpdateAsset {
  name: string;        // 文件名（如 EroLib_26.8.14_aarch64.dmg）
  url: string;         // 下载 URL
  size: number;        // 文件大小（bytes）
}
```

### UpdateProgress

```typescript
interface UpdateProgress {
  percent: number;     // 0-100
  speed: number;       // B/s
  completed: number;   // 已下载 bytes
  total: number;       // 总 bytes
}
```

## 5. 事件

| 事件 | 载荷 | 触发时机 |
|---|---|---|
| `update://progress` | `UpdateProgress` | aria2 下载进度回调 |

## 6. 检查流程

```
1. 请求 GitHub releases Atom feed:
   GET https://github.com/{owner}/{repo}/releases.atom

2. 解析最新 entry:
   - <title> → 版本号
   - <link rel="enclosure"> → 下载 URL
   - <content> → release notes

3. 比对版本号:
   current = tauri.conf.json version（如 "26.8.14+0015"）
   latest = feed 中最新 entry 的 title
   hasUpdate = latest > current

4. 返回 UpdateInfo
```

## 7. 下载流程

```
1. 前端调 download_update(url, name)
2. 后端走 aria2 下载（复用代理检测）
3. aria2 进度回调 → emit update://progress
4. 下载完成 → 返回本地路径
5. 前端显示「安装」按钮
```

## 8. 安装流程

### macOS
- 下载 .dmg → 挂载 → 替换 .app → 重启

### Windows
- 下载 .msi → `quit_and_install` 启动安装程序 → 应用退出

## 9. 前端行为

- `App.vue` `onMounted` → `updateStore.checkForUpdate()`（静默）
- 有更新 → 弹出 `UpdateDialog`
- 用户点击「下载」→ 进度条显示
- 下载完成 → 按钮变为「安装」
- 点击「安装」→ `quitAndInstall`

## 10. 版本号格式

- 格式：`YY.M.D+HHmm`（如 `26.8.14+0015` = 2026年8月14日 00:15）
- 语义化且单调递增
- 比对规则：字符串比较（日期格式天然有序）

## 11. 约束

- 检查更新必须静默（不打扰用户）
- 下载必须走 aria2（复用代理检测 + 断点续传）
- 版本号从 `tauri.conf.json` 读取，不硬编码
- Atom feed 无需 GitHub API token（公开仓库匿名可访问）
- macOS 安装需要用户确认（不自动替换）
- Windows 安装会退出应用（需提前保存状态）

## 12. 相关模块

- [module-task.md](./module-task.md) — 任务系统（复用 aria2 下载管线）
