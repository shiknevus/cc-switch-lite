import { useCallback, useMemo } from "react";
import { useSettingsQuery, useUpdateSettingsMutation } from "@/lib/query";
import type { Settings } from "@/types";
import { toast } from "@/lib/toast";

export interface SettingsFormState extends Settings {}

export function useSettings() {
  const { data: rawSettings, isLoading } = useSettingsQuery();
  const updateMutation = useUpdateSettingsMutation();

  const settings = useMemo<SettingsFormState | null>(() => {
    if (!rawSettings) return null;
    return rawSettings;
  }, [rawSettings]);

  const autoSaveSettings = useCallback(
    async (updates: Partial<Settings>) => {
      try {
        const merged: Settings = {
          ...(rawSettings || {}),
          ...updates,
        };
        await updateMutation.mutateAsync(merged);
        return { requiresRestart: false };
      } catch (e: any) {
        toast.error("保存设置失败: " + (e?.message || e));
        return null;
      }
    },
    [rawSettings, updateMutation],
  );

  return {
    settings,
    isLoading,
    autoSaveSettings,
  };
}

