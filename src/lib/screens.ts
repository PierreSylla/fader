import { invoke } from "@tauri-apps/api/core";

export type Status = "ok" | "busy" | "unreachable" | "permission";
export type BackendKind = "backlight" | "ddc" | "none";

export interface Screen {
  id: string;
  label: string;
  geometry: { x: number; y: number; width: number; height: number };
  backend: { kind: BackendKind };
  value: number | null;
  applied: number | null;
  status: Status;
  reason: string | null;
}

export interface ScreenList {
  ready: boolean;
  screens: Screen[];
}

export const listScreens = (): Promise<ScreenList> => invoke("list_screens");
export const setBrightness = (id: string, value: number): Promise<void> =>
  invoke("set_brightness", { id, value });
export const panelIsOpen = (): Promise<boolean> => invoke("panel_is_open");
export const hidePanel = (): Promise<void> => invoke("hide_panel");
export const resizePanel = (width: number, height: number): Promise<void> =>
  invoke("resize_panel", { width, height });

export const METHOD: Record<BackendKind, string> = {
  backlight: "BL",
  ddc: "DDC",
  none: "—",
};

export const METHOD_LONG: Record<BackendKind, string> = {
  backlight: "backlight",
  ddc: "DDC/CI",
  none: "no method",
};
