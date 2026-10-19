// 用量统计相关的类型定义

export interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
}

export interface RequestLog {
  requestId: string;
  providerId: string;
  providerName?: string;
  appType: string;
  model: string;
  requestModel?: string;
  /** 计费模型：命中定价表时记录的 model 键（别名 + request 重写后实际命中 model 的值） */
  pricingModel?: string;
  costMultiplier: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  inputCostUsd: string;
  outputCostUsd: string;
  cacheReadCostUsd: string;
  cacheCreationCostUsd: string;
  totalCostUsd: string;
  isStreaming: boolean;
  latencyMs: number;
  /** 真实请求返回首字耗时（毫秒）：仅对路由服务流式请求有效；按时间戳估算的会话日志不填（见 dataSource） */
  firstTokenMs?: number;
  durationMs?: number;
  statusCode: number;
  errorMessage?: string;
  createdAt: number;
  dataSource?: string;
}

export interface SessionSyncResult {
  imported: number;
  skipped: number;
  filesScanned: number;
  suspectedDuplicates: number;
  deferredFiles: number;
  errors: string[];
}

export interface PaginatedLogs {
  data: RequestLog[];
  total: number;
  page: number;
  pageSize: number;
}

export interface ModelPricing {
  modelId: string;
  displayName: string;
  inputCostPerMillion: string;
  outputCostPerMillion: string;
  cacheReadCostPerMillion: string;
  cacheCreationCostPerMillion: string;
}

export interface ModelsDevSyncConfig {
  autoSyncEnabled: boolean;
  includeCommonModels: boolean;
  selectedModelKeys: string[];
  excludedCommonModelKeys: string[];
  lastSyncAt: number | null;
  lastSyncError: string | null;
}

export interface ModelsDevSyncState {
  config: ModelsDevSyncConfig;
  configPath: string;
}

export interface UsageSummary {
  totalRequests: number;
  totalCost: string;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCacheCreationTokens: number;
  totalCacheReadTokens: number;
  /** 成功请求数（status 2xx）。汇总聚合时直接求和，避免从成功率反推的舍入误差。 */
  successCount: number;
  successRate: number;
  /** input + output + cache_creation + cache_read, all cache-normalized */
  realTotalTokens: number;
  /** cache_read / (input + cache_creation + cache_read), range 0~1 */
  cacheHitRate: number;
}

export interface UsageSummaryByApp {
  appType: string;
  summary: UsageSummary;
}

export interface DailyStats {
  date: string;
  requestCount: number;
  totalCost: string;
  totalTokens: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCacheCreationTokens: number;
  totalCacheReadTokens: number;
}

export interface ProviderStats {
  providerId: string;
  providerName: string;
  appType: string;
  requestCount: number;
  totalTokens: number;
  totalCost: string;
  avgCostPerRequest: string;
  successRate: number;
  /** 精确测速输出 token（仅路由代理成功且 ≥ 100 token 的请求） */
  speedOutputTokens?: number;
  /** 精确测速耗时毫秒（每条请求的 latencyMs - firstTokenMs 求和） */
  speedGenerationMs?: number;
  /** 估算测速输出 token（会话日志成功且 ≥ 100 token 的请求） */
  estSpeedOutputTokens?: number;
  /** 估算测速耗时毫秒（每条会话请求的估算耗时求和） */
  estSpeedDurationMs?: number;
  /** 首字时间中位数（毫秒）：仅对路由代理流式请求统计，无数据为 null */
  firstTokenMedianMs?: number | null;
  /** 会话首字时间中位数（毫秒）：会话日志根据时间戳估算首字，无数据为 null */
  estFirstTokenMedianMs?: number | null;
}

export interface ModelStats {
  model: string;
  requestCount: number;
  /** 真实全部 Tokens（输入 + 输出 + 缓存读取 + 缓存写入） */
  totalTokens: number;
  totalCost: string;
  avgCostPerRequest: string;
  successRate: number;
  /** 精确测速指标（口径与 ProviderStats 一致） */
  speedOutputTokens?: number;
  speedGenerationMs?: number;
  estSpeedOutputTokens?: number;
  estSpeedDurationMs?: number;
  /** 首字时间中位数，口径与 ProviderStats 一致 */
  firstTokenMedianMs?: number | null;
  estFirstTokenMedianMs?: number | null;
}

export interface LogFilters {
  appType?: string;
  providerName?: string;
  model?: string;
  statusCode?: number;
  startDate?: number;
  endDate?: number;
}

/**
 * Dashboard 范围过滤状态，用于 Hero / 趋势图 / 日志列表等 Tab：
 *
 * - `providerName` 用于过滤特定供应商，由 Provider 统计表或下拉选择触发
 * - `model` 用于过滤模型名称
 */
export interface UsageScopeFilters {
  appType?: string;
  providerName?: string;
  model?: string;
}

export type UsageRangePreset =
  | "today"
  | "1d"
  | "7d"
  | "14d"
  | "30d"
  | "all"
  | "custom";

export interface UsageRangeSelection {
  preset: UsageRangePreset;
  customStartDate?: number;
  customEndDate?: number;
  /** When true (custom mode only), endDate resolves to "now" instead of the
   *  fixed customEndDate snapshot, and the end-time field becomes read-only. */
  liveEndTime?: boolean;
}

/**
 * App types surfaced as dashboard filter buttons.
 *
 * `claude-desktop` is intentionally NOT listed: the Desktop gateway's proxy
 * traffic is still recorded under its own `app_type` (preserving route-takeover
 * billing audit - the request detail panel shows the real value), but the
 * dashboard folds it into `claude` for display.
 */
export type AppType =
  | "claude"
  | "codex"
  | "gemini"
  | "grokbuild"
  | "opencode"
  | "pi"
  | "mcode"
  | "dsh";

export type AppTypeFilter = "all" | AppType;

export const KNOWN_APP_TYPES: ReadonlyArray<AppType> = [
  "claude",
  "codex",
  "gemini",
  "grokbuild",
  "opencode",
  "pi",
  "mcode",
  "dsh",
];

/**
 * App types whose proxy uses an OpenAI-style protocol. Two consequences:
 *
 * 1. `inputTokens` already includes the cached portion (must subtract
 *    `cacheReadTokens` to get fresh-input semantics - see [getFreshInputTokens]).
 * 2. The protocol does not report cache _creation_ separately, only cache
 *    _reads_. So `cacheCreationTokens` is always 0 for these app types and
 *    the UI should label it as N/A rather than 0.
 *
 * Mirror of the Rust `CACHE_INCLUSIVE_APP_TYPES` whitelist.
 */
export const CACHE_INCLUSIVE_APP_TYPES: ReadonlySet<string> = new Set([
  "codex",
  "gemini",
  "grokbuild",
  "dsh",
]);

// Pi sessions can mix Anthropic and OpenAI APIs, but the dashboard aggregates
// only by app type. Treat cache-write coverage as partial without changing
// Pi's fresh-input token semantics.
const PARTIAL_CACHE_WRITE_APP_TYPES: ReadonlySet<string> = new Set([
  "pi",
  "mcode",
  "dsh",
]);

export type CacheWriteAvailability = "ok" | "partial" | "na";

export function getCacheWriteAvailability(
  appTypes: readonly string[],
): CacheWriteAvailability {
  if (appTypes.length === 0) return "ok";
  const unavailable = appTypes.filter((appType) =>
    CACHE_INCLUSIVE_APP_TYPES.has(appType),
  ).length;
  if (unavailable === appTypes.length) return "na";
  const partial = appTypes.some((appType) =>
    PARTIAL_CACHE_WRITE_APP_TYPES.has(appType),
  );
  return unavailable === 0 && !partial ? "ok" : "partial";
}

/** Subset of request-log fields needed to derive cache-normalized input. */
export interface CacheNormalizableLog {
  appType: string;
  inputTokens: number;
  cacheReadTokens: number;
}

/**
 * For a single request log, return the input token count with cache reads
 * removed. Anthropic-style providers already report `inputTokens` without
 * cache, so they pass through unchanged.
 */
export function getFreshInputTokens(log: CacheNormalizableLog): number {
  if (
    CACHE_INCLUSIVE_APP_TYPES.has(log.appType) &&
    log.inputTokens >= log.cacheReadTokens
  ) {
    return log.inputTokens - log.cacheReadTokens;
  }
  return log.inputTokens;
}

export const NON_NEGATIVE_DECIMAL_REGEX = /^\d+(?:\.\d+)?$/;

export function isNonNegativeDecimalString(value: string): boolean {
  const trimmed = value.trim();
  if (!NON_NEGATIVE_DECIMAL_REGEX.test(trimmed)) return false;
  return Number.isFinite(Number(trimmed));
}

type UsageCostLog = Pick<
  RequestLog,
  | "inputTokens"
  | "outputTokens"
  | "cacheReadTokens"
  | "cacheCreationTokens"
  | "totalCostUsd"
  | "statusCode"
> &
  Partial<Pick<RequestLog, "costMultiplier">>;

export function hasUsageTokens(log: UsageCostLog): boolean {
  return (
    log.inputTokens > 0 ||
    log.outputTokens > 0 ||
    log.cacheReadTokens > 0 ||
    log.cacheCreationTokens > 0
  );
}

export function isUnpricedUsage(log: UsageCostLog): boolean {
  const totalCost = Number.parseFloat(log.totalCostUsd);
  const multiplier =
    log.costMultiplier == null
      ? undefined
      : Number.parseFloat(log.costMultiplier);
  return (
    log.statusCode >= 200 &&
    log.statusCode < 300 &&
    hasUsageTokens(log) &&
    Number.isFinite(totalCost) &&
    (!Number.isFinite(multiplier) || multiplier !== 0) &&
    totalCost === 0
  );
}

export interface StatsFilters {
  timeRange: UsageRangePreset;
  providerId?: string;
  appType?: string;
}
