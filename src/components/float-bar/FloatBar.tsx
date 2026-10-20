import React, { useState, useEffect, useLayoutEffect, useRef, useMemo, useCallback } from "react";
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

  // 显式维护当前窗口语言状态，确保双向广播时即时渲染
  const [currentLang, setCurrentLang] = useState<string>(() => {
    if (typeof window !== "undefined") {
      try {
        const stored = window.localStorage.getItem("language");
        if (stored) return stored;
      } catch {}
    }
    return i18n.language || "zh";
  });

  // 本地即时维护 pinned 状态，避免点击图钉按钮等待 IPC 导致的界面卡死感
  const [pinned, setPinned] = useState<boolean>(
    () => settings?.floatBarPinned ?? false,
  );
  useEffect(() => {
    if (settings?.floatBarPinned !== undefined) {
      setPinned(settings.floatBarPinned);
    }
  }, [settings?.floatBarPinned]);

  const autoHide = settings?.floatBarAutoHide ?? true;
  const [expanded, setExpanded] = useState<boolean>(() => settings?.floatBarPinned ?? false);
  const [isSyncing, setIsSyncing] = useState<boolean>(false);
  const collapseTimerRef = useRef<NodeJS.Timeout | null>(null);

  // 统计范围模式：全部(all) / 今日(today) / 本次(session)
  const [rangeMode, setRangeMode] = useState<"all" | "today" | "session">(() => {
    if (typeof window !== "undefined") {
      try {
        const stored = window.localStorage.getItem("cc_float_range_mode");
        if (stored === "all" || stored === "today" || stored === "session") {
          return stored;
        }
      } catch {}
    }
    return "today";
  });

  // “本次”会话计数的起始时间（秒级 UNIX 时间戳），从选择的那一刻起算
  const [sessionStartTime, setSessionStartTime] = useState<number>(() => {
    if (typeof window !== "undefined") {
      try {
        const stored = window.sessionStorage.getItem("cc_float_session_start");
        if (stored) {
          const parsed = parseInt(stored, 10);
          if (!isNaN(parsed) && parsed > 0) return parsed;
        }
      } catch {}
    }
    return Math.floor(Date.now() / 1000);
  });

  // 格式化本次起始时刻（如 14:25:30）
  const sessionStartTimeFormatted = useMemo(() => {
    if (!sessionStartTime) return "";
    const d = new Date(sessionStartTime * 1000);
    const pad = (n: number) => n.toString().padStart(2, "0");
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  }, [sessionStartTime]);

  // 构造响应式统计范围
  const activeRange = useMemo<UsageRangeSelection>(() => {
    if (rangeMode === "all") {
      return { preset: "all" };
    }
    if (rangeMode === "session") {
      return {
        preset: "custom",
        customStartDate: sessionStartTime,
        liveEndTime: true,
      };
    }
    return { preset: "today" };
  }, [rangeMode, sessionStartTime]);

  // 选择统计范围模式（如果再次点击本次，重置起始时刻为当前并从 0 计数）
  const handleSelectMode = (mode: "all" | "today" | "session", e?: React.MouseEvent) => {
    e?.stopPropagation();
    if (mode === "session") {
      const nowSec = Math.floor(Date.now() / 1000);
      setSessionStartTime(nowSec);
      try {
        window.sessionStorage.setItem("cc_float_session_start", String(nowSec));
      } catch {}
    }
    setRangeMode(mode);
    try {
      window.localStorage.setItem("cc_float_range_mode", mode);
    } catch {}
  };

  const { data: summary } = useUsageSummary(activeRange);
  const { data: appSummaries } = useUsageSummaryByApp(activeRange);

  // 计算当前范围内的数值
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

  // 根据当前语言确定请求次数单位（中文: 次, 英文: reqs, 日文: 回）
  const activeLang = (currentLang || i18n.language || "zh").toLowerCase();
  const isEn = activeLang.startsWith("en");
  const isJa = activeLang.startsWith("ja");

  const reqUnit = useMemo(() => {
    if (isEn) return "reqs";
    if (isJa) return "回";
    return "次";
  }, [isEn, isJa]);

  // 胶囊 DOM 引用，用于动态精确测量真实排版宽度
  const capsuleRef = useRef<HTMLDivElement>(null);
  const [measuredCapsuleWidth, setMeasuredCapsuleWidth] = useState<number>(0);

  // 监听胶囊 DOM 真实渲染宽度（毫秒级自适应文字与徽章变化）
  useLayoutEffect(() => {
    const el = capsuleRef.current;
    if (!el) return;

    const measure = () => {
      // 真实无挤压的完整物理排版宽度
      const rectWidth = el.getBoundingClientRect().width;
      const w = Math.ceil(Math.max(rectWidth, el.offsetWidth, el.scrollWidth));
      if (w > 100) {
        setMeasuredCapsuleWidth((prev) => (Math.abs(prev - w) >= 2 ? w : prev));
      }
    };

    measure();

    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver(() => {
        measure();
      });
      ro.observe(el);
    }

    return () => {
      ro?.disconnect();
    };
  }, [activeApps, summary, currentLang, totalCostNum, totalTokensNum, totalReqsNum, rangeMode]);

  // 基于实际内容的特征预估宽度（首帧或 DOM 尚未完成测量时的充裕瞬间兜底，宁宽勿窄，防止挤压）
  const contentEstimatedWidth = useMemo(() => {
    // 1. 左侧三态分段按钮组(全部/今日/本次，约110~130px) + 间距(8px) + 分割线(1px) + 间距(8px)
    const selectorWidth = isEn ? 130.0 : isJa ? 120.0 : 112.0;
    const todaySection = selectorWidth + 17.0;

    // 2. 今日花费：图标(14) + gap(4) + text-xs 加粗字符宽度(每个约8px) + gap
    const costStr = money(totalCostNum, 2);
    const costSection = 18.0 + costStr.length * 8.0;

    // 3. Token 用量：分割线与间距(17) + 图标(14) + gap(4) + 字符宽度 + 'tk'(18)
    const tokenStr = formatTokensCompact(totalTokensNum);
    const tokenSection = 17.0 + 18.0 + tokenStr.length * 8.0 + 18.0;

    // 4. 请求次数：分割线与间距(17) + 图标(14) + gap(4) + 字符宽度 + 单位
    const reqStr = String(totalReqsNum);
    const unitWidth = isEn ? 32.0 : 18.0;
    const reqSection = 17.0 + 18.0 + reqStr.length * 8.0 + unitWidth;

    // 5. 各工具徽章宽度
    let appsSection = 0;
    if (activeApps.length > 0) {
      appsSection += 17.0; // 分割线与间距
      for (const app of activeApps) {
        const appCostStr = money(app.cost, 2);
        // 图标(12) + gap(4) + text-xs 金额 + padding(14) + border(2) + 间距(6)
        appsSection += 12.0 + 4.0 + appCostStr.length * 8.0 + 16.0 + 6.0;
      }
    }

    // 6. 右侧 4 个操作按钮组：分割线与间距(17) + 4 个按钮(各约 24px) + gap(4px*3) + 呼吸留白(6)
    const actionsSection = 17.0 + (4 * 24.0 + 12.0) + 6.0;

    // 7. 胶囊两端充裕内边距 ps-3 (12px) + pe-3.5 (14px) + 边框 = 28px
    const capsulePadding = 28.0;

    return Math.ceil(
      todaySection +
        costSection +
        tokenSection +
        reqSection +
        appsSection +
        actionsSection +
        capsulePadding,
    );
  }, [isEn, isJa, totalCostNum, totalTokensNum, totalReqsNum, activeApps, money]);

  // 最终动态展开窗口宽度（胶囊宽度 + 20px 安全呼吸边距，左右各留 10px 透明缓冲区，彻底杜绝窗口裁剪边缘）
  const dynamicExpandedWidth = useMemo(() => {
    const rawWidth =
      measuredCapsuleWidth > 150 ? measuredCapsuleWidth : contentEstimatedWidth;
    return Math.max(380.0, Math.ceil(rawWidth + 20.0));
  }, [measuredCapsuleWidth, contentEstimatedWidth]);

  // 动态花费金额标题提示
  const costTitle = useMemo(() => {
    if (rangeMode === "session") {
      return isEn
        ? `Session Cost (since ${sessionStartTimeFormatted})`
        : `本次支出（自 ${sessionStartTimeFormatted} 起）`;
    }
    if (rangeMode === "all") {
      return isEn ? "All Time Cost" : "全部历史支出";
    }
    return isEn ? "Today's Cost" : "今日支出";
  }, [rangeMode, sessionStartTimeFormatted, isEn]);

  // 同步 Rust 窗口大小与位置（带自适应紧凑宽度）
  const syncWindowExpanded = useCallback(
    (nextExpanded: boolean, nextPinned?: boolean) => {
      const isPinnedEffective = nextPinned !== undefined ? nextPinned : pinned;
      void invoke("set_float_bar_expanded", {
        expanded: nextExpanded,
        pinned: isPinnedEffective,
        customWidth: nextExpanded ? dynamicExpandedWidth : undefined,
      }).catch((e) => {
        console.debug("Failed to set float bar expanded state:", e);
      });
    },
    [pinned, dynamicExpandedWidth],
  );

  // 当工具数据导致自适应宽度变化且当前正处于展开状态时，无缝同步窗口尺寸
  useEffect(() => {
    if (expanded) {
      syncWindowExpanded(true);
    }
  }, [dynamicExpandedWidth]); // eslint-disable-line react-hooks/exhaustive-deps

  // 更新语言逻辑（直接内存更新，彻底阻断 invalidateQueries 造成的无限循环）
  const applyLanguage = useCallback(
    async (lang?: string) => {
      if (!lang) return;
      setCurrentLang((prev) => (prev === lang ? prev : lang));
      syncLanguageFromSettings(lang);
      if (i18n.language !== lang) {
        try {
          await i18n.changeLanguage(lang);
        } catch (err) {
          console.debug("Failed to change language:", err);
        }
      }
      queryClient.setQueryData(settingsKeys.all, (old: Settings | undefined) => {
        if (!old) return old;
        return { ...old, language: lang };
      });
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

  // 组件首次挂载时直接从 SQLite 调取配置，校准语言状态
  useEffect(() => {
    void invoke<Settings>("get_settings")
      .then((s) => {
        if (s?.language) {
          void applyLanguage(s.language);
        }
      })
      .catch((err) => console.debug("Failed to query settings on float bar mount:", err));
  }, [applyLanguage]);

  // 监听 settings 中语言字段的变动（带判等守卫）
  useEffect(() => {
    if (settings?.language && settings.language !== currentLang) {
      void applyLanguage(settings.language);
    }
  }, [settings?.language, currentLang, applyLanguage]);

  // 监听全通道语言切换广播事件（全局广播 + 针对本窗口的定向事件）
  useEffect(() => {
    let unlistenGlobal: (() => void) | undefined;
    let unlistenWin: (() => void) | undefined;

    const handler = (event: { payload?: { language?: string } }) => {
      const nextLang = event.payload?.language;
      if (nextLang && nextLang !== currentLang) {
        void applyLanguage(nextLang);
      }
    };

    void listen<{ language: string }>("app-language-changed", handler).then((fn) => {
      unlistenGlobal = fn;
    });

    try {
      const win = getCurrentWindow();
      void win
        .listen<{ language: string }>("app-language-changed", handler)
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
  }, [currentLang, applyLanguage]);

  // 当外部 pinned 改变时联动展开状态
  useEffect(() => {
    if (pinned && !expanded) {
      setExpanded(true);
      syncWindowExpanded(true, true);
    }
  }, [pinned, expanded, syncWindowExpanded]);

  // 鼠标移入指示条：立即展开
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
    if (pinned || !autoHide) return;
    if (collapseTimerRef.current) {
      clearTimeout(collapseTimerRef.current);
    }
    collapseTimerRef.current = setTimeout(() => {
      setExpanded(false);
      syncWindowExpanded(false);
      collapseTimerRef.current = null;
    }, 280);
  };

  // 切换固定图钉：即时本地乐观更新，绝不卡死
  const togglePin = (e: React.MouseEvent) => {
    e.stopPropagation();
    const nextPinned = !pinned;
    setPinned(nextPinned);
    if (nextPinned) {
      setExpanded(true);
      syncWindowExpanded(true, true);
    } else {
      syncWindowExpanded(expanded, false);
    }
    void autoSaveSettings({ floatBarPinned: nextPinned });
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

  // 隐藏/关闭悬浮栏：立即响应隐藏，绝无任何等待
  const handleHideBar = (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const win = getCurrentWindow();
      void win.hide();
    } catch (err) {
      console.debug("Float bar direct hide error:", err);
    }
    void invoke("toggle_float_bar", { enabled: false }).catch(() => {});
  };

  return (
    <div
      className="w-full h-full flex flex-col items-center justify-start select-none relative overflow-visible font-sans"
      onMouseLeave={handleMouseLeave}
    >
      {/* 1. 贴顶微小感应指示条（隐藏状态下仅在这块范围内才悬浮展开，支持双击直接打开主面板） */}
      <div
        data-tauri-drag-region
        onMouseEnter={handleMouseEnter}
        onDoubleClick={handleOpenMain}
        className={`absolute top-0 flex items-center justify-center transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] ${
          expanded
            ? "opacity-0 pointer-events-none -translate-y-2"
            : "opacity-100 pointer-events-auto translate-y-0"
        }`}
      >
        <div
          data-tauri-drag-region
          onMouseEnter={handleMouseEnter}
          onDoubleClick={handleOpenMain}
          className="h-2.5 w-44 rounded-b-full px-2 flex items-center justify-center gap-1.5 cursor-pointer bg-zinc-900/95 dark:bg-zinc-900/95 light:bg-zinc-100 border-b border-x border-purple-500/30 shadow-none drop-shadow-none hover:h-3.5 hover:w-48 transition-all duration-200"
          title={t("floatBar.hoverHint", {
            lng: currentLang,
            defaultValue: isEn
              ? "Hover to reveal, double-click to open main dashboard"
              : "悬停弹出用量条，双击打开主控制面板",
          })}
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

      {/* 2. 悬浮展开的精巧胶囊数据条（支持双击打开主窗口，宽度紧凑贴合内容，完全无阴影） */}
      <div
        data-tauri-drag-region
        onMouseEnter={handleMouseEnter}
        onDoubleClick={handleOpenMain}
        className={`absolute top-1 flex justify-center transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] ${
          expanded
            ? "opacity-100 pointer-events-auto translate-y-0 scale-100"
            : "opacity-0 pointer-events-none -translate-y-6 scale-95"
        }`}
      >
        <div
          ref={capsuleRef}
          data-tauri-drag-region
          onDoubleClick={handleOpenMain}
          className="h-[40px] w-max rounded-full ps-3 pe-3.5 flex items-center gap-2 bg-zinc-950/95 dark:bg-zinc-950/95 light:bg-white border border-white/12 dark:border-white/12 light:border-zinc-300 shadow-none drop-shadow-none text-zinc-100 dark:text-zinc-100 light:text-zinc-800"
          title={t("floatBar.capsuleHint", {
            lng: currentLang,
            defaultValue: isEn ? "Double-click to open main dashboard" : "双击打开主控制面板",
          })}
        >
          {/* 左侧：范围选择按钮组（全部 / 今日 / 本次） */}
          <div
            className="flex items-center p-0.5 rounded-full bg-white/5 dark:bg-white/5 light:bg-black/5 border border-white/8 dark:border-white/8 light:border-black/5 shrink-0"
            style={NO_DRAG}
          >
            {/* 全部 */}
            <button
              type="button"
              onClick={(e) => handleSelectMode("all", e)}
              className={`px-2 py-0.5 rounded-full text-[11px] font-medium transition-all shrink-0 outline-none ${
                rangeMode === "all"
                  ? "bg-sky-500/25 text-sky-300 dark:text-sky-300 light:text-sky-600 font-semibold shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 light:text-zinc-500 light:hover:text-zinc-800 hover:bg-white/5"
              }`}
              title={t("floatBar.rangeAllHint", {
                lng: currentLang,
                defaultValue: isEn ? "All time usage statistics" : "全部历史累计总用量",
              })}
            >
              {t("common.all", { lng: currentLang, defaultValue: isEn ? "All" : isJa ? "全期間" : "全部" })}
            </button>

            {/* 今日 */}
            <button
              type="button"
              onClick={(e) => handleSelectMode("today", e)}
              className={`px-2 py-0.5 rounded-full text-[11px] font-medium transition-all shrink-0 outline-none ${
                rangeMode === "today"
                  ? "bg-purple-500/25 text-purple-300 dark:text-purple-300 light:text-purple-600 font-semibold shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 light:text-zinc-500 light:hover:text-zinc-800 hover:bg-white/5"
              }`}
              title={t("floatBar.rangeTodayHint", {
                lng: currentLang,
                defaultValue: isEn ? "Today's usage statistics" : "今日用量统计（当天 00:00 至今）",
              })}
            >
              {t("floatBar.today", { lng: currentLang, defaultValue: isEn ? "Today" : "今日" })}
            </button>

            {/* 本次 */}
            <button
              type="button"
              onClick={(e) => handleSelectMode("session", e)}
              className={`px-2 py-0.5 rounded-full text-[11px] font-medium transition-all shrink-0 outline-none flex items-center gap-1 ${
                rangeMode === "session"
                  ? "bg-amber-500/25 text-amber-300 dark:text-amber-300 light:text-amber-600 font-semibold shadow-sm"
                  : "text-zinc-400 hover:text-zinc-200 light:text-zinc-500 light:hover:text-zinc-800 hover:bg-white/5"
              }`}
              title={
                rangeMode === "session"
                  ? t("floatBar.rangeSessionActiveHint", {
                      lng: currentLang,
                      time: sessionStartTimeFormatted,
                      defaultValue: isEn
                        ? `Since ${sessionStartTimeFormatted}. Click again to reset starting time`
                        : `自 ${sessionStartTimeFormatted} 起开始实时计数，再次点击可重置起点`,
                    })
                  : t("floatBar.rangeSessionHint", {
                      lng: currentLang,
                      defaultValue: isEn
                        ? "Start counting usage from current moment"
                        : "从选择起开始实时计数本次用量",
                    })
              }
            >
              {rangeMode === "session" && (
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 animate-pulse" />
              )}
              <span>
                {t("floatBar.session", { lng: currentLang, defaultValue: isEn ? "Session" : isJa ? "今回" : "本次" })}
              </span>
            </button>
          </div>

          <div className="h-3 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />

          {/* 中间核心统计区（紧凑排列，无冗余空隙，支持双击弹出主窗口） */}
          <div
            data-tauri-drag-region
            onDoubleClick={handleOpenMain}
            className="flex items-center gap-2 shrink-0 px-0.5 cursor-grab"
          >
            {/* 花费金额 */}
            <div
              className="flex items-center gap-1 shrink-0"
              title={costTitle}
            >
              <Coins className="h-3.5 w-3.5 text-emerald-400 dark:text-emerald-400 light:text-emerald-600 shrink-0" />
              <span className="font-mono text-xs font-bold tracking-tight text-emerald-400 dark:text-emerald-400 light:text-emerald-600">
                {money(totalCostNum, 2)}
              </span>
            </div>

            <div className="h-3 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />

            {/* Token 用量 */}
            <div
              className="flex items-center gap-1 shrink-0"
              title={t("usage.tokens", {
                lng: currentLang,
                defaultValue: isEn ? "Total Tokens" : "Token 总量",
              })}
            >
              <Cpu className="h-3.5 w-3.5 text-indigo-400 dark:text-indigo-400 light:text-indigo-600 shrink-0" />
              <span className="font-mono text-xs font-semibold">
                {formatTokensCompact(totalTokensNum)}
              </span>
              <span className="text-[10px] text-zinc-400 dark:text-zinc-400 light:text-zinc-500">
                tk
              </span>
            </div>

            <div className="h-3 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />

            {/* 请求次数（多语言单位支持） */}
            <div
              className="flex items-center gap-1 shrink-0"
              title={t("usage.requests", {
                lng: currentLang,
                defaultValue: isEn ? "Total Requests" : "调用请求数",
              })}
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
                <div className="h-3 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />
                <div className="flex items-center gap-1 shrink-0">
                  {activeApps.map((app) => (
                    <div
                      key={app.appType}
                      className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-white/5 dark:bg-white/5 light:bg-black/5 border border-white/8 dark:border-white/8 light:border-black/5 font-mono shrink-0"
                      title={`${app.appType.toUpperCase()}: ${money(app.cost, 2)} · ${formatTokensCompact(app.tokens)} tokens · ${app.requests} ${reqUnit}`}
                    >
                      <AppGlyph app={app.appType as AppId} size={12} />
                      <span className="text-xs font-semibold text-zinc-300 dark:text-zinc-300 light:text-zinc-700">
                        {money(app.cost, 2)}
                      </span>
                    </div>
                  ))}
                </div>
              </>
            )}
          </div>

          {/* 独立分割线：确保工具徽章和右侧按钮绝对隔离 */}
          <div className="h-3 w-px bg-white/10 dark:bg-white/10 light:bg-black/10 shrink-0" />

          {/* 右侧：快捷操作按钮（独立定宽定高 h-6 w-6，防挤压 shrink-0，带 no-drag，绝无重叠或遮挡） */}
          <div className="flex items-center gap-1 shrink-0 pe-0.5" style={NO_DRAG}>
            {/* 立即同步 */}
            <button
              type="button"
              onClick={handleSyncNow}
              disabled={isSyncing}
              className={`h-6 w-6 flex items-center justify-center rounded-full hover:bg-white/10 dark:hover:bg-white/10 light:hover:bg-black/10 text-zinc-400 hover:text-zinc-100 transition-colors shrink-0 ${
                isSyncing ? "text-purple-400" : ""
              }`}
              title={t("floatBar.syncNow", {
                lng: currentLang,
                defaultValue: isEn ? "Sync latest session usage now" : "立即同步最新会话用量",
              })}
            >
              <RefreshCw className={`h-3.5 w-3.5 ${isSyncing ? "animate-spin" : ""}`} />
            </button>

            {/* 固定 / 取消固定（不隐藏按钮） */}
            <button
              type="button"
              onClick={togglePin}
              className={`h-6 w-6 flex items-center justify-center rounded-full hover:bg-white/10 dark:hover:bg-white/10 light:hover:bg-black/10 transition-colors shrink-0 ${
                pinned
                  ? "text-purple-400 bg-purple-500/20"
                  : "text-zinc-400 hover:text-zinc-100"
              }`}
              title={
                pinned
                  ? t("floatBar.unpin", {
                      lng: currentLang,
                      defaultValue: isEn ? "Unpin float bar (auto-hide to top edge)" : "取消固定（恢复贴边隐藏）",
                    })
                  : t("floatBar.pin", {
                      lng: currentLang,
                      defaultValue: isEn ? "Pin float bar (keep expanded)" : "固定常驻显示（不自动收起）",
                    })
              }
            >
              {pinned ? <Pin className="h-3.5 w-3.5" /> : <PinOff className="h-3.5 w-3.5" />}
            </button>

            {/* 展开完整面板 */}
            <button
              type="button"
              onClick={handleOpenMain}
              className="h-6 w-6 flex items-center justify-center rounded-full hover:bg-white/10 dark:hover:bg-white/10 light:hover:bg-black/10 text-zinc-400 hover:text-zinc-100 transition-colors shrink-0"
              title={t("floatBar.openFullDashboard", {
                lng: currentLang,
                defaultValue: isEn ? "Expand full usage dashboard" : "展开用量统计主窗口",
              })}
            >
              <Maximize2 className="h-3.5 w-3.5" />
            </button>

            {/* 关闭/隐藏 */}
            <button
              type="button"
              onClick={handleHideBar}
              className="h-6 w-6 flex items-center justify-center rounded-full hover:bg-red-500/20 hover:text-red-400 text-zinc-400 transition-colors shrink-0"
              title={t("floatBar.hide", {
                lng: currentLang,
                defaultValue: isEn ? "Hide float bar (re-enable via tray or settings)" : "隐藏桌面悬浮顶栏（可在托盘或设置中重新开启）",
              })}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
