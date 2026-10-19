import { QueryClient, useQuery, useMutation } from "@tanstack/react-query";
import { settingsApi } from "@/lib/api/settings";
import type { Settings } from "@/types";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 30,
      retry: 1,
    },
  },
});

export const settingsKeys = {
  all: ["settings"] as const,
};

export function useSettingsQuery() {
  return useQuery({
    queryKey: settingsKeys.all,
    queryFn: () => settingsApi.get(),
  });
}

export function useUpdateSettingsMutation() {
  return useMutation({
    mutationFn: (settings: Settings) => settingsApi.save(settings),
    onSuccess: (_, settings) => {
      queryClient.setQueryData(settingsKeys.all, settings);
    },
  });
}

