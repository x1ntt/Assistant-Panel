# Assistant Panel

个人助手面板工具集，基于 Tauri v2 + Vue3 + Rust。从屏幕右侧滑出的侧边面板。

## 技术栈

- **桌面运行时**: Tauri v2 (Rust)
- **前端**: Vue 3 + TypeScript + Vite 6
- **UI**: shadcn-vue + TailwindCSS v4 (扁平设计，圆角 2px)
- **状态管理**: Pinia + Tauri Store (JSON 持久化)
- **图标**: lucide-vue-next
- **窗口**: 3 个 Tauri 窗口 (main, floating, close-btn)

## 功能

### 已实现
- [x] 面板滑出/收起 (Ctrl+Shift+` 热键 + ESC)
- [x] 独立悬浮窗 (从左侧滑出)
- [x] 插件系统 (静态注册，Compact + 可选 FloatingView)
- [x] 设置系统 (类 VSCode JSON 配置 + 快捷键录制)
- [x] 日志系统 (system.log + 插件日志)
- [x] 端口转发 (TCP 转发 + 实时统计 + 暂停/恢复 + 持久化)
- [x] 窗口置顶 (Ctrl+Alt+` + 蓝色边框 + 开关)
- [x] 主题切换 (亮/暗)
- [x] 卡片拖拽排序 (持久化)
- [x] 退出按钮 (确认弹窗) + Alt+F4 屏蔽

### 待实现
- [ ] 进程管理 (类 pm2)
- [ ] API 信息面板
- [ ] 系统托盘

## 开发

```bash
# 前置: Rust (rustup), Node.js, pnpm, MSVC Build Tools
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

## 项目结构

```
assistant-panel/
├── src/                          # Vue3 前端
│   ├── components/               # PanelShell, ToolList, BottomBar, FloatingContent...
│   ├── components/ui/            # shadcn-vue 组件
│   ├── composables/              # useTauriCommand, useTheme, usePluginOrder, useHotkeyRecorder
│   ├── plugins/                  # port-forward, window-pin, process-manager, api-dashboard
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

## 热键

| 功能 | 默认快捷键 | 可自定义 |
|---|---|---|
| 面板收起/展开 | Ctrl+Shift+` | 是 (重启生效) |
| 窗口置顶 | Ctrl+Alt+` | 是 (重启生效) |
| 收起当前窗口 | ESC | 否 |
