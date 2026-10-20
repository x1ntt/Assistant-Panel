# Assistant Panel

个人助手面板工具集，基于 Tauri v2 + Vue3 + Rust。从屏幕右侧滑出的侧边面板。

## 技术栈

- **桌面运行时**: Tauri v2 (Rust)
- **前端**: Vue 3 + TypeScript + Vite 8（具体版本见 package.json 与锁文件）
- **UI**: shadcn-vue + TailwindCSS v4（紧凑扁平设计，圆角使用主题 tokens）
- **状态管理**: Pinia + Tauri Store (JSON 持久化)
- **图标**: lucide-vue-next
- **窗口**: 4 个 Tauri 窗口 (main, floating, close-btn, alert)

## 功能

### 已实现
- [x] 面板滑出/收起 (Ctrl+Shift+` 热键 + ESC)
- [x] 独立悬浮窗 (从左侧滑出)
- [x] 插件系统 (静态注册，Compact + 可选 FloatingView)
- [x] 设置系统 (类 VSCode JSON 配置 + 快捷键录制)
- [x] 日志系统 (system.log + 插件日志)
- [x] 端口转发 (TCP 转发 + 实时统计 + 暂停/恢复 + 持久化)
- [x] 网络流量（逐接口每秒采样、监听累计、默认三条折叠、多接口置顶、多行高亮）
- [x] 窗口置顶 (Ctrl+Alt+` + 蓝色边框 + 开关)
- [x] 主题切换 (亮/暗)
- [x] 卡片拖拽排序 (持久化)
- [x] 待办事项（两级任务、行内编辑、完成联动与可靠本地持久化）
- [x] 退出按钮 (确认弹窗) + Alt+F4 屏蔽

### 待实现
- [ ] 进程管理 (类 pm2)
- [ ] API 信息面板
- [ ] 系统托盘

## 开发

```bash
# 前置: Windows、Rust (rustup)、兼容 Vite 8 的 Node.js、pnpm、MSVC Build Tools
pnpm install

# 方式一 (推荐)
# 终端1:
pnpm dev
# 终端2:
cargo run --manifest-path src-tauri/Cargo.toml

# 方式二
pnpm tauri dev  # 可能需要先杀旧进程

# 构建
pnpm tauri build
```

Tauri 开发模式通过 `beforeDevCommand` 启动 Vite（端口 1420）。正式构建先执行 `pnpm build`（`vue-tsc --noEmit` + Vite），将前端写入 `dist/`，再编译 Rust 并生成安装包。默认 Rust 产物在 `src-tauri/target/`，安装包在 `src-tauri/target/release/bundle/`；这些产物不纳入 Git。

## 端口转发规则恢复

规则保存在应用数据目录的 `settings.json`，键为 `port-forward.rules`。重启后即使端口被占用或监听 IP 不可用，规则仍保留在列表中，显示“启动失败”和原因，可点击播放按钮重试；暂停规则保持暂停。

加载完成前禁止修改规则；配置文件损坏或读取失败时显示错误并允许重试，不以空列表覆盖配置。保存失败时显示“规则未保存”，保留当前运行状态并提供“重试保存”；成功保存前禁止继续修改，避免重复新增或覆盖待保存的更改。未保存的更改在退出应用后可能丢失。

端口转发回归测试：`node --test src/plugins/port-forward/composables/usePortForward.test.mjs`；配置解析测试：`cargo test --manifest-path src-tauri/Cargo.toml commands::port_forward::tests`。

## 网络流量

“网络流量”作为独立插件显示，表格为接口、速度、流量、操作。速度和流量均以 `↑` 发送、`↓` 接收，上下两行展示，格式与端口转发一致。

- 使用 Windows IP Helper API，全应用只有一个每秒采样任务；包括物理、虚拟、VPN、回环及未连接接口，不按有无 IP 过滤。此模块需要 Windows 10 1703 或更新版本（无统计接口枚举模式）。
- 流量是本次监听期间的累计字节，不是开机以来的系统计数。首样本建立基线，应用重启从零累计。面板隐藏或表格折叠时继续统计，插件禁用时停止；同次运行重新启用保留已累计量，但不补计停用期间流量。
- 表格默认收起，只显示筛选并排序后的前三条；底部可展开全部或收起。标题显示当前筛选后的接口数量，不再提供加号按钮。
- 默认隐藏 Windows 标记为过滤模块的接口（如 WFP、QoS、Npcap 等驱动层），正文“显示过滤接口 / 隐藏过滤接口”按钮可切换。真实网卡、WSL/VMware/TAP/TUN 等非过滤接口继续显示；按系统 `FilterInterface` 属性识别，不按名字或是否有流量猜测。标题数量显示筛选后总数，正文列出全部及隐藏数量。先筛选再取前三条，显示开关不跨重启保存。
- 隐藏只影响展示，所有接口继续采样；隐藏过滤项的置顶、高亮及累计仍保留，显示后恢复原顺序。小于 1KB/s 的速度最多一位小数并去掉末尾 `.0`，例如 `414.1B/s`、`414B/s`。
- **置顶**支持多个接口，每次点击都将目标接口升到第一条（已置顶也可再次置顶），其余置顶接口仍排在普通接口前；置顶及顺序跨重启保存，不影响累计或采样。
- 旧版移除记录已废弃：新版启动自动恢复全部可发现接口；暂时不存在的接口重新出现后自动纳入，不再提供移除或手动添加。恢复的接口从新基线计数，不补算曾移除期间的历史流量。
- **高亮**可切换并同时标记多行，不改变顺序；同次运行保留，应用重启不保存高亮。
- 不可用/采样失败显示状态，累计冻结，恢复时重建基线；计数重置不会造成负速度或巨大流量。物理和虚拟接口可能反映同一传输，因此不提供简单求和的整机总量。
- 网卡重装产生新 GUID 时视为新接口。网卡字节包含系统接口统计的流量，和端口转发只统计 TCP 转发负载的数值并不等价。
- 若 `settings.json` 或保存的置顶列表损坏，模块会保守停止采样并提示错误，不用空配置覆盖；修复配置文件后重启应用。网络插件启停也不会覆盖这份损坏配置。废弃移除键的值不参与读取校验或接口筛选。

架构上，`src/plugins/network-traffic/` 只订阅 Tauri 快照和发出操作命令，`src-tauri/src/modules/network_monitor.rs` 负责发现与采样，`commands/network_traffic.rs` 提供 IPC。无 Vue Router，也没有每个窗口独立采样。

## 待办事项

任务文字与输入使用 11px 字号。有子任务的父任务左侧箭头可展开/收起子任务，默认展开；折叠状态保留在本次运行中，重启后恢复展开，不参与任务数据同步。新增子任务会自动展开；折叠前先保存正在编辑的子任务，保存失败时保留展开与草稿。折叠不改变完成计算或级联删除范围。Todo 后端状态在 Builder 阶段注册，不依赖 setup 读取设置完成后才能首次加载。

“待办事项”默认启用，标题徽标是未删除的顶层任务总数。在设置中禁用只隐藏卡片，不清除已保存任务。

- 标题右上角 **+** 在列表末尾新增任务；父任务右侧 **+** 新增子任务，仅支持两级，不支持任务拖拽或筛选。
- 双击任务文字，或聚焦文字后按 **Enter / F2** 行内编辑。草稿按 **Enter** 或失焦提交，**Esc** 取消且不收起面板；中文输入法组合期间不提交或取消。空白新增取消，空白编辑报错且原任务不变。一次只编辑一个草稿，切换前提交；保存失败保留草稿，可再次提交，版本冲突可点击“刷新”后重试。尚未提交的草稿不持久化。
- 无子任务的父任务可手动勾选。有子任务时父任务不显示复选框，只有全部未删除子任务完成才完成；完成文字加删除线，不改变顺序。新增子任务默认未完成；删除子任务按剩余子任务重算，删除最后一个子任务保留父任务当时的有效状态并恢复复选框。
- 删除父任务会同时删除全部子任务。未完成父任务需在按钮附近的浮层再次点击“删除”；再次点击原删除按钮、“取消”、**Esc** 或等待 4 秒可取消。确认时任务状态或子任务集合变化会更新提示并要求重新确认。已完成父任务、子任务直接删除。
- 已保存数据位于 Tauri 的应用数据目录 `app_data_dir/todo.json`（Windows 默认 `%LOCALAPPDATA%\com.administrator.assistant-panel\todo.json`），独立于 Tauri Store。加载失败会显示错误与重试，不用空任务覆盖损坏文件；提交采用版本检查与同目录临时文件替换，失败保留已确认数据。
- 删除采用 `deletedAt` 标记，界面隐藏但文件保留记录，不自动清理。稳定 ID、父引用、时间与删除标记只提供未来同步的数据基础；当前没有网络同步，`revision` 仅是本地提交序号，不是跨设备冲突协议。未来 API 仍需鉴权、服务端版本/游标、离线队列、冲突与删除清理策略。

## 项目结构

```
assistant-panel/
├── src/                          # Vue3 前端
│   ├── components/               # PanelShell, ToolList, BottomBar, FloatingContent...
│   ├── components/ui/            # shadcn-vue 组件
│   ├── composables/              # useTauriCommand, useTheme, usePluginOrder, useHotkeyRecorder
│   ├── plugins/                  # todo, network-traffic, port-forward, window-pin, process-manager, api-dashboard
│   ├── stores/                   # panel.ts
│   └── lib/                      # utils.ts
├── src-tauri/                    # Rust 后端
│   └── src/
│       ├── lib.rs                # 入口 + 全局热键 + 窗口配置
│       ├── state.rs              # AppState, ForwardRule, PinnedWindow
│       ├── logger.rs             # 日志写入
│       ├── commands/             # panel, port_forward, window_pin, settings, log
│       └── modules/              # port_forwarder, window_pinner
├── todo.md                       # 当前进度
├── Record.md                     # 开发记录
└── README.md
```

## 添加新插件

1. 创建 `src/plugins/my-plugin/Compact.vue` (必须)
2. 可选: `Floating.vue` (悬浮窗内容)
3. 在 `src/plugins/registry.ts` 添加注册项

## 启动诊断

所有窗口在配置中设置 `create: false`，由 `setup()` 注册共享状态后再创建 WebView，避免首次 IPC 早于 `AppState` 注册而出现“State not managed”；仅延迟显示窗口不足以阻止 WebView 提前加载。网络 IPC 在状态尚未就绪时返回可重试错误，不触发 panic。辅助窗口在首次渲染前识别类型，避免误加载主面板插件。诊断日志会保留 Tauri“state() called before manage()”错误中的状态类型，其他动态 panic 内容仍省略。目标机启动故障是否完全消除仍需新版运行结果确认。

诊断版在 Tauri 初始化前写启动阶段和 Rust panic 日志，不依赖网络或前端成功加载。若另一台电脑启动卡死后退出，关闭旧实例、安装诊断版并启动一次，然后用 Win+R 打开：

```text
%LOCALAPPDATA%\com.administrator.assistant-panel\logs\diagnostics
```

若该目录不可写，回退位置为 `%TEMP%\assistant-panel-diagnostics`。找到最新 `startup-时间-PID.log`，提供最后约30行，发送前检查并隐藏私人路径。普通日志目录及其写入失败也会记录在诊断日志中，普通日志备用目录为 `%TEMP%\assistant-panel-logs`。

日志只帮助定位，不能捕获 Windows 加载器或所有原生访问异常。如果两个诊断目录均无日志，或日志停在窗口构建之前，请提供事件查看器“Windows日志→应用程序”的故障应用、故障模块和异常代码。不能仅因电脑离线就认定网络请求是崩溃原因。

## 热键

| 功能 | 默认快捷键 | 可自定义 |
|---|---|---|
| 面板收起/展开 | Ctrl+Shift+` | 是 (重启生效) |
| 窗口置顶 | Ctrl+Alt+` | 是 (重启生效) |
| 收起当前窗口 | ESC | 否 |
