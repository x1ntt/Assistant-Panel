import { defineAsyncComponent } from "vue";
import type { PanelPlugin } from "./types";

export const pluginRegistry: PanelPlugin[] = [
  {
    id: "port-forward",
    name: "端口转发",
    description: "TCP/UDP 端口转发",
    icon: "Network",
    category: "network",
    compactView: defineAsyncComponent(
      () => import("./port-forward/Compact.vue")
    ),
    order: 1,
    enabled: true,
  },
  {
    id: "window-pin",
    name: "窗口置顶",
    description: "窗口置顶管理",
    icon: "Pin",
    category: "window",
    compactView: defineAsyncComponent(
      () => import("./window-pin/Compact.vue")
    ),
    order: 2,
    enabled: true,
  },
  {
    id: "process-manager",
    name: "进程管理",
    description: "进程管理 (类似 pm2)",
    icon: "Activity",
    category: "system",
    compactView: defineAsyncComponent(
      () => import("./process-manager/Compact.vue")
    ),
    floatingView: defineAsyncComponent(
      () => import("./process-manager/Floating.vue")
    ),
    showFloatingButton: false,
    order: 3,
    enabled: true,
  },
  {
    id: "api-dashboard",
    name: "API 面板",
    description: "API 信息面板",
    icon: "Globe",
    category: "api",
    compactView: defineAsyncComponent(
      () => import("./api-dashboard/Compact.vue")
    ),
    floatingView: defineAsyncComponent(
      () => import("./api-dashboard/Floating.vue")
    ),
    order: 4,
    enabled: true,
  },
];
