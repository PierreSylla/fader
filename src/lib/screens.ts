import { invoke } from "@tauri-apps/api/core";

export type Status = "ok" | "busy" | "unreachable" | "permission";

export interface Screen {
  id: string;
  label: string;
  geometry: { x: number; y: number; width: number; height: number };
  backend: { kind: "backlight" | "ddc" | "none" };
  value: number | null;
  status: Status;
  reason: string | null;
}

export const listScreens = (): Promise<Screen[]> => invoke("list_screens");
export const panelIsOpen = (): Promise<boolean> => invoke("panel_is_open");
export const hidePanel = (): Promise<void> => invoke("hide_panel");
export const resizePanel = (height: number): Promise<void> =>
  invoke("resize_panel", { height });

export const METHOD: Record<Screen["backend"]["kind"], string> = {
  backlight: "BL",
  ddc: "DDC",
  none: "—",
};
