import { useCallback, useMemo, useRef } from "react";
import {
  queryClient,
  settingsKeys,
  useSettingsQuery,
  useUpdateSettingsMutation,
} from "@/lib/query";
import type { Settings } from "@/types";
import { toast } from "@/lib/toast";

export interface SettingsFormState extends Settings {}

export function useSettings() {
  const { data: rawSettings, isLoading } = useSettingsQuery();
  const updateMutation = useUpdateSettingsMutation();
  const saveQueue = useRef<Promise<void>>(Promise.resolve());

  const settings = useMemo<SettingsFormState | null>(() => {
    if (!rawSettings) return null;
    return rawSettings;
  }, [rawSettings]);

  const autoSaveSettings = useCallback(
    async (updates: Partial<Settings>) => {
      let result: { requiresRestart: false } | null = null;
      const operation = saveQueue.current.then(async () => {
        // 设置还没从后端读出来时不能保存：此时 rawSettings 是 null，
        // 用默认值合并会整包写库，把用户已有的语言/主题/刷新间隔等覆盖成默认。
        const current =
          queryClient.getQueryData<Settings>(settingsKeys.all) ?? rawSettings;
        if (!current) {
          console.warn(
            "[useSettings] 设置尚未加载完成，跳过本次保存以避免覆盖已有配置",
          );
          return;
        }
        try {
          const merged: Settings = { ...current, ...updates };
          await updateMutation.mutateAsync(merged);
          queryClient.setQueryData(settingsKeys.all, merged);
          result = { requiresRestart: false };
        } catch (error: unknown) {
          const message = error instanceof Error ? error.message : String(error);
          toast.error("保存设置失败: " + message);
        }
      });
      saveQueue.current = operation.then(
        () => undefined,
        () => undefined,
      );
      await operation;
      return result;
    },
    [rawSettings, updateMutation],
  );

  return {
    settings,
    isLoading,
    autoSaveSettings,
  };
}
