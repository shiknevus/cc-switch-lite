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

  /** 开关开机自启；`silent` 决定启动项里带不带 --silent（开机时不弹主窗口）。 */
  async setAutoLaunch(enabled: boolean, silent: boolean): Promise<boolean> {
    return await invoke("set_auto_launch", { enabled, silent });
  },

  /** 读系统里实际的自启状态（可能与设置里存的意图不一致）。 */
  async getAutoLaunchStatus(): Promise<boolean> {
    return await invoke("get_auto_launch_status");
  },
};
