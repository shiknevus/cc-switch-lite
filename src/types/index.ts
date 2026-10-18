export type AppId =
  | "claude"
  | "claude-desktop"
  | "codex"
  | "gemini"
  | "grokbuild"
  | "opencode"
  | "openclaw"
  | "hermes"
  | "pi"
  | "mcode"
  | "dsh";

export interface UsageDisplaySettings {
  currency?: string;
  exchangeRates?: Record<string, number>;
  sidebarTodayItems?: string[];
  requestLogColumns?: Record<string, boolean>;
  trendChartType?: string;
}

export interface Settings {
  language?: string;
  theme?: string;
  usageDashboardRefreshIntervalMs?: number;
  sessionAutoSyncEnabled?: boolean;
  usageDisplay?: UsageDisplaySettings;
}

export interface UsageResult {
  success: boolean;
  message?: string;
  data?: any;
}

export interface Provider {
  id: string;
  name: string;
  appType: string;
}


