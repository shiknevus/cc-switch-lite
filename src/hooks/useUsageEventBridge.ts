import { useQueryClient } from "@tanstack/react-query";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { usageKeys } from "@/lib/query/usage";

/** Invalidate usage queries when the backend imports new rows or completes a scan. */
export function useUsageEventBridge() {
  const queryClient = useQueryClient();

  useTauriEvent("usage-log-recorded", () => {
    void queryClient.invalidateQueries({ queryKey: usageKeys.all });
  });

  useTauriEvent("usage-sync-finished", () => {
    void queryClient.invalidateQueries({
      queryKey: [...usageKeys.all, "session-last-sync"],
    });
  });
}
