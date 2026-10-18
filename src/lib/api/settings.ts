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
    return await invoke("open_app_config_dir");
  },

  async setAutoLaunch(enabled: boolean, silent: boolean): Promise<boolean> {
    return await invoke("set_auto_launch", { enabled, silent });
  },

  async getAutoLaunchStatus(): Promise<boolean> {
    return await invoke("get_auto_launch_status");
  },
};
