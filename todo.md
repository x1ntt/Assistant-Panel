# Todo

## 已完成

### 第一期 — 脚手架 + 基础框架
- [x] 技术选型 (Tauri v2 + Vue3 + shadcn-vue + TailwindCSS v4)
- [x] 架构设计 (插件系统、浮动窗、设置/日志系统)
- [x] 安装 Rust 工具链 (rustc 1.98.1)
- [x] 创建 Tauri 项目 + 安装依赖
- [x] 面板滑出机制 (热键 + 窗口定位 + Rust 窗口动画)
- [x] 独立悬浮窗 (floating 窗口) + 关闭按钮 (close-btn 窗口)
- [x] 插件系统骨架 (registry.ts + types.ts + 4 个插件)
- [x] 设置系统 (SettingsView.vue + Rust get/set_setting)
- [x] 日志系统 (Logger + LogView.vue + Rust get_logs/list_log_sources)
- [x] 项目文档 (README.md, todo.md, Record.md)

### UI 重构
- [x] 扁平设计 (圆角 2px，间距压缩)
- [x] 全面中文化
- [x] 卡片设计 (小标题 + 可选浮窗按钮 + 拖拽排序)
- [x] 底栏 (设置 + 日志 + 主题切换 + 退出按钮)
- [x] 主题同步 (write_theme/read_theme + broadcast_theme 事件)
- [x] 拖拽排序 (Pointer Events + Tauri Store 持久化)

### 端口转发
- [x] Rust TCP 转发引擎 (独立线程 + Tokio 运行时)
- [x] 命令: add/remove/pause/resume/disconnect/get_forward_rules/get_interfaces
- [x] 实时统计 (每秒 emit port-forward:stats 事件)
- [x] UI: 完整表格 (监听/端口/目标/协议/连接/速度↑↓/流量↑↓/操作)
- [x] 展开子行 (客户端连接详情)
- [x] 编辑模式 (一行横排布局)
- [x] 暂停/恢复 + 持久化 (Tauri Store)
- [x] 添加行 (接口下拉 + 自定义输入)

### 窗口置顶
- [x] Win32 API (GetForegroundWindow + SetWindowPos + DwmSetWindowAttribute)
- [x] 热键 Ctrl+Alt+` (单个 global-shortcut 插件分发)
- [x] 蓝色边框 (DWMWA_BORDER_COLOR)
- [x] 开关 + PIN_ENABLED 全局标志
- [x] 主面板保护 (跳过 "Assistant Panel" 标题)
- [x] 快捷键录制组件 (HotkeyInput.vue + useHotkeyRecorder.ts)

### 窗口行为
- [x] ESC 收起 (主窗口 + 悬浮窗 + 关闭按钮)
- [x] Alt+F4 屏蔽
- [x] 退出按钮 (确认弹窗)
- [x] 单实例 (single-instance 插件)

### 进程管理
- [x] Rust 进程引擎 (process_manager.rs + commands/process.rs)
- [x] 命令: add/start/stop/remove/update/list
- [x] 状态监控 (2秒轮询 check_alive + 自动重启)
- [x] 双存储 (Tauri Store 存定义 + process_manager.json 存 pid)
- [x] 面板重启保护 (自动跳过已运行进程)
- [x] UI: 表格 (命令/工作目录/状态/开机启动/操作)
- [x] 添加/编辑行 (命令输入 + 工作目录 + 开机启动 + 崩溃重启)
- [x] 操作按钮 (Play/Stop + Pencil + Trash2)

## 待修复

- [ ] `pnpm tauri dev` 启动崩溃（exit code -1）— 需要用 `cargo run` 方式启动

## 已修复 (2026-09-15)

- [x] 窗口置顶开关状态未持久化到 Rust PIN_ENABLED — 启动时从 store 读取 `windowPin.enabled` 并同步到 `PIN_ENABLED`
- [x] 快捷键设置保存后不生效 — 改为运行时重新注册全局快捷键（unregister + register），无需重启
- [x] 主面板层级被置顶窗口覆盖 — 每次置顶操作后重新设置 main window always_on_top

## 下一步

1. 实现 API 面板插件
2. 系统托盘集成

## 关键注意事项

- **运行方式**: 先 `pnpm dev`（终端1），再 `cargo run --manifest-path src-tauri/Cargo.toml`（终端2）
- **杀进程**: 每次重新运行前 `Stop-Process -Name "assistant-panel" -Force`
- **热键**: 面板 `Ctrl+Shift+``，置顶 `Ctrl+Alt+``
- **圆角**: `--radius: 0.25rem` (2px)
- **日志**: `./logs/<timestamp>/system.log` + `<plugin-id>.log`
- **配置**: Tauri Store `settings.json`
- **悬浮窗**: 独立 Tauri 窗口 (label: "floating")，通过 emit/listen 通信
- **关闭按钮**: 独立窗口 (label: "close-btn")，32x32，在两窗之间
- **端口转发**: 独立 std::thread + Tokio Runtime（不能用 block_on）
- **所有 IPC 结构体**: `#[serde(rename_all = "camelCase")]`
