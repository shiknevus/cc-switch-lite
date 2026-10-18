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
  /** 关窗时隐藏到系统托盘继续后台同步，而不是退出进程 */
  minimizeToTrayOnClose?: boolean;
  /** 随系统启动自动运行 */
  launchOnStartup?: boolean;
  /** 开机自启拉起来时只驻留托盘，不显示主窗口 */
  silentStartup?: boolean;
  usageDisplay?: UsageDisplaySettings;
}

export interface Provider {
  id: string;
  name: string;
  appType: string;
}


