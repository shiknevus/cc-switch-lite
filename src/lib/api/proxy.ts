import { invoke } from "@tauri-apps/api/core";

export const proxyApi = {
  async getPricingModelSource(appType: string): Promise<string> {
    return invoke("get_pricing_model_source", { appType });
  },

  async setPricingModelSource(appType: string, value: string): Promise<void> {
    return invoke("set_pricing_model_source", { appType, value });
  },
};

