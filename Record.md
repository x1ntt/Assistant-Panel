# Record

## 2026-09-14 第一期 — 项目初始化

### 环境准备

- 安装 Rust: `Invoke-WebRequest rustup-init.exe` + `rustup-init.exe -y --default-toolchain stable --default-host x86_64-pc-windows-msvc`
- 版本: rustc 1.98.1, cargo 1.98.1
- pnpm 已安装 (v12.3.4)

### 创建项目

- 命令: `pnpm create tauri-app assistant-panel --template vue-ts --manager pnpm --yes`
- 工作目录: `D:\1\plane\assistant-panel`

### 安装依赖

- Tauri 插件 (逐一安装): `pnpm tauri add global-shortcut window-state positioner store shell single-instance clipboard-manager notification http process`
- 前端: `pnpm add pinia @vueuse/core nanoid tailwindcss @tailwindcss/vite lucide-vue-next`
- shadcn-vue: `npx shadcn-vue@latest init --defaults --force` + `npx shadcn-vue@latest add button card scroll-area badge separator switch input select tooltip sonner dialog tabs label`

### 问题 & 解决

- **问题**: pnpm approve-builds vue-demi 需要批准构建脚本
  **解决**: `pnpm approve-builds vue-demi`

- **问题**: TypeScript 6.0 deprecates `baseUrl` in tsconfig.json
  **解决**: 添加 `"ignoreDeprecations": "6.0"` 到 compilerOptions

- **问题**: shadcn-vue 安装的是 `@lucide/vue` 而非 `lucide-vue-next`
  **解决**: 额外安装 `pnpm add lucide-vue-next` (deprecated 但兼容)

- **问题**: Tauri capabilities 中 `positioner:allow-update-position` 权限不存在
  **解决**: 改为 `positioner:allow-move-window`

- **问题**: Rust `AppHandle::path()` 需要 `Manager` trait 在作用域
  **解决**: 在 logger.rs 和 commands/log.rs 中添加 `use tauri::Manager`

### 文件创建/修改

- `vite.config.ts` — 添加 TailwindCSS v4 插件 + @ 别名
- `tsconfig.json` — 添加 baseUrl + paths + ignoreDeprecations
- `src-tauri/tauri.conf.json` — 面板窗口配置 (decorations:false, transparent, alwaysOnTop)
- `src-tauri/Cargo.toml` — 添加 chrono, thiserror 依赖
- `src-tauri/src/lib.rs` — 全局热键注册 + 窗口定位逻辑
- `src-tauri/src/state.rs` — AppState, Settings, LogEntry
- `src-tauri/src/logger.rs` — 日志写入模块
- `src-tauri/src/commands/` — panel.rs, settings.rs, log.rs
- `src-tauri/capabilities/desktop.json` — 权限配置
- `src/App.vue` — 根组件 + Pinia
- `src/components/` — PanelShell, PanelHeader, ToolList, ToolListItem, BottomBar, FloatingPanel, SettingsView, LogView
- `src/plugins/` — types.ts, registry.ts, 4个插件 (port-forward, window-pin, process-manager, api-dashboard)
- `src/stores/panel.ts` — 面板状态 store
- `src/composables/useTauriCommand.ts` — invoke 封装

### 当前状态

- 前端构建: ✅ `pnpm build` 成功
- Rust 编译: ✅ `cargo check` 成功 (5 个 warnings)
- 集成验证: ⏳ 待 `pnpm tauri dev` 验证

## 2026-09-14 UI 重构 — 扁平化 + 中文 + 动画 + 悬浮窗

### 改动

- `tauri.conf.json` — `visible: false` → `visible: true`
- `lib.rs` — 启动时定位窗口到右边缘；热键改为 `emit("hotkey:toggle")` 事件
- `stores/panel.ts` — 新增 `isCollapsed`, `collapse()`, `expand()`
- `PanelShell.vue` — CSS 滑入/滑出动画 (`translate-x-full` ↔ `translate-x-0`)；监听 `hotkey:toggle` 事件
- `PanelHeader.vue` — 移除文字，仅保留右上角收回箭头 (ChevronRight)
- `BottomBar.vue` — Add 按钮 → 主题切换 (Moon/Sun)；中文化
- `composables/useTheme.ts` — 新增：主题切换 + 持久化到 Tauri Store
- `ToolListItem.vue` — 重构卡片：小标题 (text-[10px]) + 可选浮窗按钮 (Maximize2)
- `FloatingPanel.vue` — 独立浮窗从左侧弹出 (width: calc(100%-2rem))；圆形关闭按钮在右侧
- `plugins/types.ts` — `detailView` → `floatingView`
- `plugins/registry.ts` — 中文化 name/description；detailView → floatingView
- 所有 Compact.vue — 中文化文本
- 所有 Floating.vue — 新建（替代 Detail.vue）
- `main.css` — `--radius: 0.25rem`，`--radius-sm: 0`
- `capabilities/desktop.json` — 添加 `core:event:allow-emit`, `core:event:allow-listen`
- 安装 `@tauri-apps/plugin-store` npm 包

### 问题 & 解决

- **问题**: `core:window:allow-emit` 权限不存在
  **解决**: emit 权限在 `core:event` 下，改为 `core:event:allow-emit`

- **问题**: `@tauri-apps/plugin-store` npm 包未安装（只装了 Rust crate）
  **解决**: `pnpm add @tauri-apps/plugin-store`

### 当前状态

- 前端构建: ✅ `pnpm build` 成功
- Rust 编译: ✅ `cargo check` 成功
- 集成验证: ⏳ 待 `pnpm tauri dev` 验证

## 2026-09-14 Bug修复 — 窗口隐藏 + 悬浮窗独立窗口

### 问题

1. CSS `translate-x-full` 只移动内容，窗口本身仍可见
2. 悬浮窗与主窗口重叠，应为独立窗口

### 修复

**窗口隐藏**:
- `stores/panel.ts` — collapse() 动画结束后(320ms)调 `invoke("hide_panel")`; expand() 先 `invoke("show_panel")` 再触发动画
- `commands/panel.rs` — 新增 `show_floating` / `hide_floating` 命令

**独立悬浮窗**:
- `tauri.conf.json` — 新增 floating 窗口 (label: "floating", visible: false)
- `FloatingContent.vue` — 新建，独立窗口内容，监听 `floating:set-content` 事件
- `PanelShell.vue` — 检测 `getCurrentWebview().label` 渲染不同内容
- Pinia 跨窗口通信通过 Tauri events (`emit`/`listen`)
- 删除旧 `FloatingPanel.vue`

### 当前状态

- 前端构建: ✅
- Rust 编译: ✅

## 2026-09-14 Bug修复2 — 窗口动画 + 主题同步 + 关闭联动

### 问题

1. 窗口本身不动，只移内容，然后突然消失
2. 切换主题后悬浮窗不同步
3. 关闭主界面后悬浮窗仍显示
4. 悬浮窗关闭按钮不在两窗口之间
5. 悬浮窗开关无动画

### 修复方案

**窗口动画（改为 Rust 端动画窗口位置）**:
- `commands/panel.rs` — `slide_out_main`/`slide_in_main`: 10步 ease-out cubic 动画移动窗口位置
  - collapse: 窗口滑到屏幕右侧外 → hide
  - expand: 窗口从屏幕右侧外 show → 滑回右边缘
- `stores/panel.ts` — collapse()/expand() 直接调 invoke 命令，不再用 CSS translate
- `PanelShell.vue` — 移除 `translate-x-full` CSS 过渡

**悬浮窗动画**:
- `show_floating`: 窗口从 main 窗口位置滑出到左侧（10步250ms）
- `hide_floating`: 窗口滑回 main 窗口位置后 hide
- 悬浮窗宽度452px（420内容+32关闭按钮条）

**主题同步**:
- `useTheme.ts` — toggleTheme 后调 `invoke("broadcast_theme")` 广播到所有窗口
- `commands/panel.rs` — `broadcast_theme`: emit `theme:change` 到 main + floating
- `FloatingContent.vue` — 调用 `useTheme()` 监听 `theme:change` 事件

**关闭联动**:
- `stores/panel.ts` — collapse() 中同时关闭悬浮窗 (`invoke("hide_floating")` via `slide_out_main`)

**关闭按钮位置**:
- `FloatingContent.vue` — 布局改为 `flex`，右侧32px窄条（`w-8`）放圆形关闭按钮

### 当前状态

- 前端构建: ✅
- Rust 编译: ✅

## 2026-09-14 Bug修复3 — 独立关闭按钮窗口 + 主题同步

### 问题

1. 关闭按钮在悬浮窗内部，应独立于两个窗口之间
2. 切换深色模式后悬浮窗不变

### 修复

**独立关闭按钮窗口**:
- `tauri.conf.json` — 新增第三个窗口 `close-btn` (32x32, transparent, alwaysOnTop)
- `CloseButton.vue` — 新建，仅渲染圆形 `>` 按钮
- `PanelShell.vue` — 检测3种窗口 label: main / floating / close-btn
- `commands/panel.rs` — show_floating 后自动定位 close-btn 到两窗中间
- 悬浮窗宽度恢复 420px

**主题同步（改为 store 轮询）**:
- `FloatingContent.vue` — 500ms 轮询 Tauri Store 读取主题设置
- 移除事件广播机制（跨窗口事件不可靠）

## 2026-09-15 端口转发功能

### 实现

**Rust 后端**:
- `state.rs` — 新增 `ForwardRule`, `ConnectionStats`, `ForwardRuleSnapshot`, `NewRule`, `InterfaceInfo`
- `modules/port_forwarder.rs` — 新建，TCP 转发引擎：accept_loop + handle_tcp + stats_loop
  - `tokio::net::TcpListener` 监听 + 每连接 spawn 转发任务
  - `AtomicU64` 跟踪字节，每秒计算速度并 emit `port-forward:stats` 事件
  - `CancellationToken` 控制停止
- `commands/port_forward.rs` — 新建：get_interfaces, add_forward_rule, remove_forward_rule, disconnect_rule_connection, get_forward_rules
- `Cargo.toml` — 新增 tokio(full), tokio-util, local-ip-address, nanoid
- `state::AppState` 新增 `port_forwards: Mutex<HashMap<String, Arc<ForwardRule>>>`

**Vue 前端**:
- `port-forward/Compact.vue` — 重写：完整表格 UI（监听/端口/目标/协议/连接/速度/流量/操作）
- `port-forward/composables/usePortForward.ts` — 新建：规则管理 + 实时统计监听 + 格式化工具
- `registry.ts` — 移除 port-forward 的 floatingView

### 当前状态

- 前端构建: ✅
- Rust 编译: ✅

## 2026-09-15 窗口置顶功能

### 实现

**Rust 后端**:
- `Cargo.toml` — 新增 `windows` crate (Win32_UI_WindowsAndMessaging, Win32_Foundation, Win32_Graphics_Dwm)
- `state.rs` — 新增 `PinnedWindow` + `pinned_windows: Mutex<HashMap>`
- `modules/window_pinner.rs` — Win32 API: GetForegroundWindow, SetWindowPos, DwmSetWindowAttribute
- `commands/window_pin.rs` — toggle_pin_focused, unpin_window, unpin_all, list_pinned
- `lib.rs` — 注册 `Ctrl+Alt+Backquote` 热键

**Vue 前端**:
- `composables/useHotkeyRecorder.ts` — 通用快捷键录制
- `components/HotkeyInput.vue` — 快捷键输入框
- `plugins/window-pin/Compact.vue` — 开关 + 置顶列表 + 快捷键编辑

### 当前状态

- 前端构建: ✅
- Rust 编译: ✅

## 2026-09-15 窗口置顶修复

- 合并为单个 global-shortcut 插件，handler 内部区分分发
- 新增 `update_hotkey` 命令支持动态修改快捷键
- SettingsView 使用 HotkeyInput 组件 + 中文化
- DWM 边框颜色只在焦点时显示（Win11 限制）

## 2026-09-15 窗口置顶修复2

### 修复内容

1. **主面板保护** — `toggle_pin_focused` 接受 `skip_titles` 参数，跳过 "Assistant Panel"
2. **边框改回蓝色** — `0x00FF9000` (BGR)
3. **PIN_ENABLED 全局标志** — `AtomicBool`，热键 handler 检查此标志
4. **开关持久化** — 前端 `onMounted` 调 `set_pin_enabled` 同步状态
5. **快捷键从 store 加载** — setup 中读取 `panel.hotkey` 和 `windowPin.hotkey`，带 fallback
6. **重启生效提示** — SettingsView 和 window-pin Compact 中标注"重启生效"

### 关键架构决策

- 热键 handler 是闭包，无法访问 AppState → 用全局 `static PIN_ENABLED: AtomicBool`
- `toggle_pin_focused_inner` 是无 State 版本，供热键 handler 调用
- 快捷键变更只保存到 store，下次启动读取并注册（不支持运行时热更新）

### 已知问题

- `pnpm tauri dev` 启动崩溃（exit code -1）— 旧进程残留导致 single-instance 退出
- 解决：每次运行前 `Stop-Process -Name "assistant-panel" -Force`
- 或用两终端方式：`pnpm dev` + `cargo run --manifest-path src-tauri/Cargo.toml`

## 2026-09-15 窗口置顶修复3 — 开关 + 快捷键 + 主面板层级

### 修复内容

1. **开关持久化** — 启动时从 store 读取 `windowPin.enabled` 同步到 `PIN_ENABLED`；`set_pin_enabled` 同时写入 store
2. **快捷键运行时生效** — 全局快捷键 handler 改为从 `Mutex<Option<Shortcut>>` 全局变量读取当前快捷键；`update_hotkey` 命令改为 unregister 旧快捷键 + register 新快捷键 + 更新全局变量，无需重启
3. **主面板层级保护** — `toggle_pin_focused` / `toggle_pin_focused_inner` 每次置顶操作后调用 `main_win.set_always_on_top(true)` 保持主面板在最上层
4. **Switch 组件修复** — reka-ui `SwitchRoot` 只 emit `update:modelValue` 不 emit `update:checked`，3 处 Switch 组件全部改为 `:model-value` + `@update:model-value`

### 关键架构决策

- `Shortcut` 是 `HotKey` 别名，实现了 `PartialEq`，handler 内直接用 `==` 比较
- 全局快捷键存储在 `static PANEL_SHORTCUT: Mutex<Option<Shortcut>>` 中
- 热键注册改为：handler 读取全局变量比较，`update_hotkey` 时 unregister 旧的 + register 新的

### 当前状态

- 前端构建: ✅
- Rust 编译: ✅

## 2026-09-15 进程管理插件

### 实现

**Rust 后端**:
- `state.rs` — 新增 `ProcessEntry` + `PidMapEntry` + `AppState.processes: Mutex<HashMap>`
- `modules/process_manager.rs` — 核心引擎：parse_command, start_process, stop_process, check_alive, kill_process_tree, startup_auto_processes, save/load_pid_map, monitor_process
- `commands/process.rs` — 6 个 IPC 命令：add_process, start_process_cmd, stop_process, remove_process, update_process, list_processes
- `lib.rs` — setup 中调 `startup_auto_processes`；invoke_handler 注册 6 个新命令

**Vue 前端**:
- `process-manager/composables/useProcessManager.ts` — 新建：进程管理 composable
- `process-manager/Compact.vue` — 重写：完整表格 UI（命令/工作目录/状态/开机启动/操作）

### 关键架构决策

- **双存储**：Tauri Store 保存进程定义（command/working_dir/auto_start/auto_restart），`process_manager.json` 保存 pid 映射
- **重启保护**：面板启动时从 pid map 加载 pid，用 `tasklist` 检查是否存活，存活则重连（不重复启动）
- **进程监控**：每个运行中进程一个 monitor 线程，2 秒轮询 `check_alive(pid)`，退出时 emit 事件
- **崩溃重启**：`auto_restart` 独立于 `auto_start`，崩溃后延迟 1 秒自动重启
- **进程树终止**：Windows 用 `taskkill /F /T /PID` 杀整个进程树
- **命令解析**：支持引号包裹的参数（split_command 函数）
