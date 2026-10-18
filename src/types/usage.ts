// 浣跨敤缁熻鐩稿叧绫诲瀷瀹氫箟

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
  /** 鍐欏叆鏃跺疄闄呯敤浜庤浠风殑妯″瀷鍚嶏紱璺敱鎺ョ + request 璁′环妯″紡涓嬪彲鑳戒笌 model 涓嶅悓 */
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
  /** 璺敱鏈嶅姟璁扮殑鏄簿纭瀛楋紱浼氳瘽鏃ュ織瀵煎叆鐨勬槸鎸夋椂闂存埑鎺ㄥ嚭鏉ョ殑浼扮畻鍊硷紙鐪?dataSource锛?*/
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

export interface DataSourceSummary {
  dataSource: string;
  requestCount: number;
  totalCostUsd: string;
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
  successRate: number;
  /** input + output + cache_creation + cache_read, all cache-normalized */
  realTotalTokens: number;
  /** cache_read / (input + cache_creation + cache_read), range 0鈥? */
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
  appType: string;
  providerName: string;
  requestCount: number;
  /** 鐪熷疄娑堣€?Tokens锛堟柊澧炶緭鍏?+ 杈撳嚭 + 缂撳瓨鍐欏叆 + 缂撳瓨鍛戒腑锛夛紝涓庢寚鏍囧崱鍚屽彛寰?*/
  totalTokens: number;
  totalCost: string;
  successRate: number;
  avgLatencyMs: number;
  /** 閫熷害鍒嗗瓙锛氭湁棣栧瓧銆佽緭鍑?鈮?100 token 鐨勬槑缁嗚姹傜殑杈撳嚭涔嬪拰锛堟棩姹囨€讳笉璁★級 */
  speedOutputTokens?: number;
  /** 閫熷害鍒嗘瘝锛氬悓涓€鎵硅姹傜殑锛堣€楁椂 鈭?棣栧瓧锛変箣鍜岋紝姣 */
  speedGenerationMs?: number;
  /** 浼扮畻閫熷害鍒嗗瓙锛氫細璇濇棩蹇楀鍏ャ€佹湁浼扮畻鑰楁椂銆佽緭鍑?鈮?200 token 鐨勮姹傜殑杈撳嚭涔嬪拰 */
  estSpeedOutputTokens?: number;
  /** 浼扮畻閫熷害鍒嗘瘝锛氬悓涓€鎵硅姹傜殑浼扮畻鑰楁椂锛堝惈棣栧瓧绛夊緟锛変箣鍜岋紝姣 */
  estSpeedDurationMs?: number;
  /** 棣栧瓧鏃堕棿涓綅鏁帮紙姣锛夛細缁忚矾鐢辨湇鍔°€佹垚鍔熺殑娴佸紡鏄庣粏璇锋眰锛涙病鏈夋椂涓?null */
  firstTokenMedianMs?: number | null;
  /** 浼扮畻棣栧瓧涓綅鏁帮紙姣锛夛細浼氳瘽鏃ュ織瀵煎叆銆佹寜鏃堕棿鎴虫帹鍑轰簡棣栧瓧鐨勮姹傦紱娌℃湁绮剧‘鍊兼椂鎵嶆樉绀猴紝甯?鈮?*/
  estFirstTokenMedianMs?: number | null;
}

export interface ModelStats {
  model: string;
  requestCount: number;
  /** 鐪熷疄娑堣€?Tokens锛堟柊澧炶緭鍏?+ 杈撳嚭 + 缂撳瓨鍐欏叆 + 缂撳瓨鍛戒腑锛夛紝涓庢寚鏍囧崱鍚屽彛寰?*/
  totalTokens: number;
  totalCost: string;
  avgCostPerRequest: string;
  successRate: number;
  /** 閫熷害鍒嗗瓙鍒嗘瘝锛屽彛寰勫悓 ProviderStats 鐨勫洓涓€熷害瀛楁 */
  speedOutputTokens?: number;
  speedGenerationMs?: number;
  estSpeedOutputTokens?: number;
  estSpeedDurationMs?: number;
  /** 棣栧瓧鏃堕棿涓綅鏁帮紝鍙ｅ緞鍚?ProviderStats 鐨勯偅涓や釜瀛楁 */
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
 * Dashboard 椤舵爮鐨勫叏灞€绛涢€夌淮搴︼紝浣滅敤浜?Hero / 瓒嬪娍鍥?/ 涓変釜缁熻 Tab銆? *
 * - `providerName` 鎸夊睍绀哄悕绮剧‘鍖归厤锛堜笌 Provider 缁熻鍒楄〃鍚屽彛寰勶紝鍚? *   "Claude (Session)" 绛変細璇濆崰浣嶅悕锛夛紱
 * - `model` 鎸夈€屾湁鏁堣浠锋ā鍨嬨€嶅尮閰嶏紙pricing_model 浼樺厛銆佸洖钀?model锛? *   涓庢ā鍨嬬粺璁＄殑鍒嗙粍鍙ｅ緞涓€鑷达級銆? */
export interface UsageScopeFilters {
  appType?: string;
  providerName?: string;
  model?: string;
}

export interface ProviderLimitStatus {
  providerId: string;
  dailyUsage: string;
  dailyLimit?: string;
  dailyExceeded: boolean;
  monthlyUsage: string;
  monthlyLimit?: string;
  monthlyExceeded: boolean;
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
 * billing audit 鈥?the request detail panel shows the real value), but the
 * dashboard folds it into `claude` for display. It is the embedded Claude Code
 * runtime running inside the Desktop shell, and Desktop *chat* usage never
 * passes through this app at all, so a separate "Claude Desktop" bucket would
 * only ever show a partial number and mislead users into reading it as the
 * Desktop's full usage. The backend collapses `claude-desktop 鈫?claude` in
 * every dashboard query (see `folded_app_type_sql`).
 * `opencode` and `pi` have no proxy handler; their usage reaches this
 * dashboard through session importers. `openclaw` / `hermes` appear only as
 * managed apps elsewhere.
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
  "pi",
  "dsh",
];

/**
 * App types whose proxy uses an OpenAI-style protocol. Two consequences:
 *
 * 1. `inputTokens` already includes the cached portion (must subtract
 *    `cacheReadTokens` to get fresh-input semantics 鈥?see
 *    [getFreshInputTokens]).
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


