import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "@/types";

export const settingsApi = {
  async get(): Promise<Settings> {
    return await invoke("get_settings");
  },

  async save(settings: Settings): Promise<boolean> {
    return await invoke("save_settings", { settings });
  },

  async openAppConfigFolder(): Promise<void> {
    return await invoke("open_dir");
  },
};
