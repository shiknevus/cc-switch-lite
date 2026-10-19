import React, { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Coins,
  Cpu,
  Layers,
  Maximize2,
  Pin,
  PinOff,
  RefreshCw,
  X,
} from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useMoneyFormat } from "@/hooks/useMoneyFormat";
import { useUsageEventBridge } from "@/hooks/useUsageEventBridge";
import { useUsageSummary, useUsageSummaryByApp } from "@/lib/query/usage";
import { queryClient, settingsKeys } from "@/lib/query";
import { formatTokensCompact } from "@/components/usage/format";
import { AppGlyph } from "@/components/shell/AppGlyph";
import { syncLanguageFromSettings } from "@/i18n";
import type { AppId, Settings } from "@/types";
import type { UsageRangeSelection } from "@/types/usage";
import { usageApi } from "@/lib/api/usage";

const NO_DRAG = { WebkitAppRegion: "no-drag" } as React.CSSProperties;

export function FloatBar() {
  const { t, i18n } = useTranslation();
  const { settings, autoSaveSettings } = useSettings();
  const { money } = useMoneyFormat();
  useUsageEventBridge();

  // 显式维护当前窗口语言状态，确保双向广播时百分之百强制局部与全局即时刷新
  const [currentLang, setCurrentLang] = useState<string>(() => {
    if (typeof window !== "undefined") {
      try {
        const stored = window.localStorage.getItem("language");
        if (stored) return stored;
      } catch {}
    }
    return i18n.language || "zh";
  });

  const isPinned = settings?.floatBarPinned ?? false;
  const autoHide = settings?.floatBarAutoHide ?? true;

  const [expanded, setExpanded] = useState<boolean>(isPinned);
  const [isSyncing, setIsSyncing] = useState<boolean>(false);
  const collapseTimerRef = useRef<NodeJS.Timeout | null>(null);

  // 更新语言的统一执行逻辑
  const applyLanguage = useCallback(
    async (lang?: string) => {
      if (!lang) return;
      setCurrentLang(lang);
      syncLanguageFromSettings(lang);
      try {
        await i18n.changeLanguage(lang);
      } catch (err) {
        console.debug("Failed to change language:", err);
      }
      void queryClient.invalidateQueries({ queryKey: settingsKeys.all });
    },
    [i18n],
  );

  // 默认软件启动时显示悬浮窗
  useEffect(() => {
    if (settings?.floatBarEnabled !== false) {
      try {
        const win = getCurrentWindow();
        void win.show();
        void win.setAlwaysOnTop(true);
      } catch (e) {
        console.debug("Float bar show error:", e);
      }
    }
  }, [settings?.floatBarEnabled]);

  // 组件挂载时直接主动向后端查询一次最新设置（避免冷启动时由于 React Query 缓存尚未填充导致语言滞后）
  useEffect(() => {
    void invoke<Settings>("get_settings")
      .then((s) => {
        if (s?.language) {
          void applyLanguage(s.language);
        }
      })
      .catch((err) => console.debug("Failed to query settings on float bar mount:", err));
  }, [applyLanguage]);

  // 同步主窗口与后端的语言配置（来自 SQLite settings）
  useEffect(() => {
    if (settings?.language) {
      void applyLanguage(settings.language);
    }
  }, [settings?.language, applyLanguage]);

  // 监听 i18n 自身语言切换回调
  useEffect(() => {
    const handleLangChange = (lng: string) => {
      setCurrentLang(lng);
    };
    i18n.on("languageChanged", handleLangChange);
    return () => {
      i18n.off("languageChanged", handleLangChange);
    };
  }, [i18n]);

  // 监听全通道语言切换广播事件（全局广播 + 针对本窗口的定向事件）
  useEffect(() => {
    let unlistenGlobal: (() => void) | undefined;
    let unlistenWin: (() => void) | undefined;

    void listen<{ language: string }>("app-language-changed", (event) => {
      const nextLang = event.payload?.language;
      if (nextLang) {
        void applyLanguage(nextLang);
      }
    }).then((fn) => {
      unlistenGlobal = fn;
    });

    try {
      const win = getCurrentWindow();
      void win
        .listen<{ language: string }>("app-language-changed", (event) => {
          const nextLang = event.payload?.language;
          if (nextLang) {
            void applyLanguage(nextLang);
          }
        })
        .then((fn) => {
          unlistenWin = fn;
        });
    } catch (e) {
      console.debug("Float bar window listener registration error:", e);
    }

    return () => {
      unlistenGlobal?.();
      unlistenWin?.();
    };
  }, [applyLanguage]);

  // 根据当前语言确定请求次数单位（中文: 次, 英文: reqs, 日文: 回）
  const activeLang = (currentLang || i18n.language || "zh").toLowerCase();
  const isEn = activeLang.startsWith("en");
  const isJa = activeLang.startsWith("ja");

  const reqUnit = useMemo(() => {
    if (isEn) return "reqs";
    if (isJa) return "回";
    return "次";
  }, [isEn, isJa]);

  // 今日统计范围
  const todayRange = useMemo<UsageRangeSelection>(
    () => ({ preset: "today" }),
    [],
  );

  const { data: summary } = useUsageSummary(todayRange);
  const { data: appSummaries } = useUsageSummaryByApp(todayRange);

  // 计算今日数值
  const totalCostNum = summary ? parseFloat(summary.totalCost) || 0 : 0;
  const totalTokensNum = summary?.realTotalTokens ?? 0;
  const totalReqsNum = summary?.totalRequests ?? 0;

  // 严格从左到右按花费金额降序排列（花费最高排在最左侧）
  const activeApps = useMemo(() => {
    if (!appSummaries) return [];
    return appSummaries
      .map((item) => ({
        appType: item.appType,
        cost: parseFloat(item.summary.totalCost) || 0,
        tokens: item.summary.realTotalTokens,
        requests: item.summary.totalRequests,
      }))
      .filter((item) => item.requests > 0 || item.tokens > 0 || item.cost > 0)
      .sort((a, b) => b.cost - a.cost || b.tokens - a.tokens)
      .slice(0, 4);
  }, [appSummaries]);

  // 同步 Rust 窗口大小与位置
  const syncWindowExpanded = useCallback((nextExpanded: boolean) => {
    void invoke("set_float_bar_expanded", { expanded: nextExpanded }).catch(
      (e) => {
        console.debug("Failed to set float bar expanded state:", e);
      },
    );
  }, []);

  // 当外部 pinned 改变时联动展开状态
  useEffect(() => {
    if (isPinned && !expanded) {
      setExpanded(true);
      syncWindowExpanded(true);
    }
  }, [isPinned, expanded, syncWindowExpanded]);

  // 鼠标移入指示条（只有进入该隐藏小条范围才触发）：立即展开
  const handleMouseEnter = () => {
    if (collapseTimerRef.current) {
      clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }
    if (!expanded) {
      setExpanded(true);
      syncWindowExpanded(true);
    }
  };

  // 鼠标移出整个胶囊：如果未锁定且开启自动贴边隐藏，延迟收起回微小指示条
  const handleMouseLeave = () => {
    if (isPinned || !autoHide) return;
    if (collapseTimerRef.current) {
      clearTimeout(collapseTimerRef.current);
    }
    collapseTimerRef.current = setTimeout(() => {
      setExpanded(false);
      syncWindowExpanded(false);
      collapseTimerRef.current = null;
    }, 280);
  };

  // 切换固定图钉
  const togglePin = async (e: React.MouseEvent) => {
    e.stopPropagation();
    const nextPinned = !isPinned;
    await autoSaveSettings({ floatBarPinned: nextPinned });
    if (nextPinned && !expanded) {
      setExpanded(true);
      syncWindowExpanded(true);
    }
  };

  // 打开主控制面板
  const handleOpenMain = (e: React.MouseEvent) => {
    e.stopPropagation();
    void invoke("open_main_window_from_float");
  };

  // 立即同步会话日志
  const handleSyncNow = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isSyncing) return;
    setIsSyncing(true);
    try {
      await usageApi.syncSessionUsage();
    } catch (err) {
      console.warn("Manual sync from float bar failed:", err);
    } finally {
      setTimeout(() => {
        setIsSyncing(false);
      }, 600);
    }
  };

  // 隐藏/关闭悬浮栏
  const handleHideBar = (e: React.MouseEvent) => {
    e.stopPropagation();
    void invoke("toggle_float_bar", { enabled: false });
  };

  return (
    <div
      className="w-full h-full flex flex-col items-center justify-start select-none relative overflow-visible font-sans"
      onMouseLeave={handleMouseLeave}
    >
      {/* 1. 贴顶微小感应指示条（隐藏状态下仅在这块范围内才悬浮展开） */}
      <div
        data-tauri-drag-region
        onMouseEnter={handleMouseEnter}
        className={`absolute top-0 flex items-center justify-center transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] ${
          expanded
            ? "opacity-0 pointer-events-none -translate-y-2"
            : "opacity-100 pointer-events-auto translate-y-0"
        }`}
      >
        <div
          data-tauri-drag-region
          onMouseEnter={handleMouseEnter}
          className="h-2.5 w-44 rounded-b-full px-2 flex items-center justify-center gap-1.5 cursor-pointer backdrop-blur-xl bg-zinc-900/90 dark:bg-zinc-900/90 light:bg-zinc-100/95 border-b border-x border-purple-500/30 shadow-[0_4px_16px_rgba(168,85,247,0.3)] hover:h-3.5 hover:w-48 hover:shadow-[0_6px_20px_rgba(168,85,247,0.5)] transition-all duration-200"
          title={t("floatBar.hoverHint", { lng: currentLang, defaultValue: isEn ? "Hover to reveal usage statistics" : "悬停弹出用量统计条" })}
        >
          <div className="flex items-center gap-1.5 pointer-events-none">
            <span className="h-1.5 w-1.5 rounded-full bg-purple-400 animate-pulse" />
            <span className="text-[9px] font-mono tracking-wider font-semibold text-purple-300 opacity-90 uppercase">
              CC SWITCH
            </span>
            <span className="h-1.5 w-1.5 rounded-full bg-indigo-400 animate-pulse" />
          </div>
        </div>
      </div>

      {/* 2. 悬浮展开的精巧胶囊数据条 */}
      <div
        data-tauri-drag-region
        onMouseEnter={handleMouseEnter}
        className={`absolute top-1.5 transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] ${
          expanded
            ? "opacity-100 pointer-events-auto translate-y-0 scale-100"
            : "opacity-0 pointer-events-none -translate-y-6 scale-95"
        }`}
      >
        <div
          data-tauri-drag-region
          className="h-[44px] w-[718px] rounded-full px-3 flex items-center justify-between gap-2.5 backdrop-blur-2xl bg-zinc-950/90 dark:bg-zinc-950/90 light:bg-white/95 border border-white/12 dark:border-white/12 light:border-zinc-300 shadow-[0_12px_40px_rgba(0,0,0,0.45)] text-zinc-100 dark:text-zinc-100 light:text-zinc-800"
        >
          {/* 左侧：仅「今日」标识，去掉左侧小图标，点击打开主面板 */}
          <button
            type="button"
            className="flex items-center cursor-pointer shrink-0 outline-none"
            onClick={handleOpenMain}
            title={t("floatBar.openMain", { lng: currentLang, defaultValue: isEn ? "Open main dashboard" : "点击打开用量主面板" })}
            style={NO_DRAG}
          >
            <span className="text-[11px] font-medium px-2 py-0.5 rounded-full bg-purple-500/15 text-purple-400 dark:text-purple-300 light:text-purple-600 border border-purple-500/25 hover:bg-purple-500/25 transition-colors">
              {t("floatBar.today", { lng: currentLang, defaultValue: isEn ? "Today" : "今日" })}
            </span>
          </button>

          {/* 中间核心统计区（整行可拖拽，原汁原味极简风） */}
          <div
            data-tauri-drag-region
            className="flex items-center gap-2.5 flex-1 justify-center min-w-0 px-1 cursor-grab"
          >
            {/* 今日花费 */}
            <div
              className="flex items-center gap-1 shrink-0"
              title={t("usage.cost", { lng: currentLang, defaultValue: isEn ? "Today's Cost" : "今日支出" })}
            >
              <Coins className="h-3.5 w-3.5 text-emerald-400 dark:text-emerald-400 light:text-emerald-600 shrink-0" />
              <span className="font-mono text-sm font-bold tracking-tight text-emerald-400 dark:text-emerald-400 light:text-emerald-600">
                {money(totalCostNum, 2)}
              </span>
            </div>

            <div className="h-3.5 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />

            {/* Token 用量 */}
            <div
              className="flex items-center gap-1 shrink-0"
              title={t("usage.tokens", { lng: currentLang, defaultValue: isEn ? "Total Tokens" : "Token 总量" })}
            >
              <Cpu className="h-3.5 w-3.5 text-indigo-400 dark:text-indigo-400 light:text-indigo-600 shrink-0" />
              <span className="font-mono text-xs font-semibold">
                {formatTokensCompact(totalTokensNum)}
              </span>
              <span className="text-[10px] text-zinc-400 dark:text-zinc-400 light:text-zinc-500">
                tk
              </span>
            </div>

            <div className="h-3.5 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />

            {/* 请求次数（多语言单位支持） */}
            <div
              className="flex items-center gap-1 shrink-0"
              title={t("usage.requests", { lng: currentLang, defaultValue: isEn ? "Total Requests" : "调用请求数" })}
            >
              <Layers className="h-3.5 w-3.5 text-sky-400 dark:text-sky-400 light:text-sky-600 shrink-0" />
              <span className="font-mono text-xs font-medium">
                {totalReqsNum}
              </span>
              <span className="text-[10px] text-zinc-400 dark:text-zinc-400 light:text-zinc-500">
                {reqUnit}
              </span>
            </div>

            {/* 各工具徽章：从左到右严格按花费金额降序排列 */}
            {activeApps.length > 0 && (
              <>
                <div className="h-3.5 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />
                <div className="flex items-center gap-1.5 shrink-0">
                  {activeApps.map((app) => (
                    <div
                      key={app.appType}
                      className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-white/5 dark:bg-white/5 light:bg-black/5 border border-white/8 dark:border-white/8 light:border-black/5 text-[10px] font-mono shrink-0"
                      title={`${app.appType.toUpperCase()}: ${money(app.cost, 2)} · ${formatTokensCompact(app.tokens)} tokens · ${app.requests} ${reqUnit}`}
                    >
                      <AppGlyph app={app.appType as AppId} size={13} />
                      <span className="font-semibold text-zinc-300 dark:text-zinc-300 light:text-zinc-700">
                        {money(app.cost, 2)}
                      </span>
                    </div>
                  ))}
                </div>
              </>
            )}
          </div>

          {/* 独立分割线：确保工具徽章和右侧按钮绝对隔离 */}
          <div className="h-3.5 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />

          {/* 右侧：快捷操作按钮（独立成组，带 no-drag，绝无重叠） */}
          <div className="flex items-center gap-0.5 shrink-0" style={NO_DRAG}>
            {/* 立即同步 */}
            <button
              type="button"
              onClick={handleSyncNow}
              disabled={isSyncing}
              className={`p-1.5 rounded-full hover:bg-white/10 dark:hover:bg-white/10 light:hover:bg-black/10 text-zinc-400 hover:text-zinc-100 transition-colors shrink-0 ${
                isSyncing ? "text-purple-400" : ""
              }`}
              title={t("floatBar.syncNow", { lng: currentLang, defaultValue: isEn ? "Sync latest session usage now" : "立即同步最新会话用量" })}
            >
              <RefreshCw className={`h-3.5 w-3.5 ${isSyncing ? "animate-spin" : ""}`} />
            </button>

            {/* 固定 / 取消固定 */}
            <button
              type="button"
              onClick={togglePin}
              className={`p-1.5 rounded-full hover:bg-white/10 dark:hover:bg-white/10 light:hover:bg-black/10 transition-colors shrink-0 ${
                isPinned
                  ? "text-purple-400 bg-purple-500/20"
                  : "text-zinc-400 hover:text-zinc-100"
              }`}
              title={
                isPinned
                  ? t("floatBar.unpin", { lng: currentLang, defaultValue: isEn ? "Unpin float bar (auto-hide to top edge)" : "取消固定（恢复贴边隐藏）" })
                  : t("floatBar.pin", { lng: currentLang, defaultValue: isEn ? "Pin float bar (keep expanded)" : "固定常驻显示（不自动收起）" })
              }
            >
              {isPinned ? <Pin className="h-3.5 w-3.5" /> : <PinOff className="h-3.5 w-3.5" />}
            </button>

            {/* 展开完整面板 */}
            <button
              type="button"
              onClick={handleOpenMain}
              className="p-1.5 rounded-full hover:bg-white/10 dark:hover:bg-white/10 light:hover:bg-black/10 text-zinc-400 hover:text-zinc-100 transition-colors shrink-0"
              title={t("floatBar.openFullDashboard", { lng: currentLang, defaultValue: isEn ? "Expand full usage dashboard" : "展开用量统计主窗口" })}
            >
              <Maximize2 className="h-3.5 w-3.5" />
            </button>

            {/* 关闭/隐藏 */}
            <button
              type="button"
              onClick={handleHideBar}
              className="p-1.5 rounded-full hover:bg-red-500/20 hover:text-red-400 text-zinc-400 transition-colors shrink-0"
              title={t("floatBar.hide", { lng: currentLang, defaultValue: isEn ? "Hide float bar (re-enable via tray or settings)" : "隐藏桌面悬浮顶栏（可在托盘或设置中重新开启）" })}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
