import type { InjectionKey, Ref } from "vue";

/**
 * Card header integration.
 *
 * Each plugin's compact view can push a count badge and an "add" action up into
 * the shared card title bar (rendered by ToolListItem), so the compact body
 * doesn't need its own header row — saving vertical space.
 */
export interface CardHeaderOptions {
  /** Reactive count shown as a badge next to the title. */
  count?: Ref<number>;
  /** If provided, an add (+) button appears in the title bar. */
  onAdd?: () => void;
}

export interface CardHeaderApi {
  register: (opts: CardHeaderOptions) => void;
}

export const CardHeaderKey: InjectionKey<CardHeaderApi> = Symbol("cardHeader");
