import type { Component } from "vue";

export type PluginCategory = "network" | "window" | "system" | "api" | "dev";

export interface PanelPlugin {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: PluginCategory;
  compactView: Component;
  floatingView?: Component;
  /** When false, the title bar won't show the maximize/floating button
   *  (the plugin opens its floating window through its own UI instead). */
  showFloatingButton?: boolean;
  order: number;
  enabled: boolean;
}
