import { useCallback, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
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
  const saveQueue = useRef<Promise<boolean>>(Promise.resolve(true));
  const { t } = useTranslation();

  const settings = useMemo<SettingsFormState | null>(() => {
    if (!rawSettings) return null;
    return rawSettings;
  }, [rawSettings]);

  const autoSaveSettings = useCallback(
    async (updates: Partial<Settings>): Promise<boolean> => {
      let success = false;
      const operation = saveQueue.current.then(async () => {
        // 设置还没从后端读出来时不能保存：此时 rawSettings 是 null，
        // 用默认值合并会整包写库，把用户已有的语言/主题/刷新间隔等覆盖成默认。
        const current =
          queryClient.getQueryData<Settings>(settingsKeys.all) ?? rawSettings;
        if (!current) {
          console.warn(
            "[useSettings] 设置尚未加载完成，跳过本次保存以避免覆盖已有配置",
          );
          return false;
        }
        try {
          const merged: Settings = { ...current, ...updates };
          await updateMutation.mutateAsync(merged);
          queryClient.setQueryData(settingsKeys.all, merged);
          success = true;
          return true;
        } catch (error: unknown) {
          const message = error instanceof Error ? error.message : String(error);
          toast.error(
            t("settings.saveFailed", {
              defaultValue: "保存设置失败: {{error}}",
              error: message,
            }),
          );
          return false;
        }
      });
      saveQueue.current = operation.then(
        (ok) => ok,
        () => false,
      );
      await operation;
      return success;
    },
    [rawSettings, updateMutation, t],
  );

  return {
    settings,
    isLoading,
    autoSaveSettings,
  };
}

