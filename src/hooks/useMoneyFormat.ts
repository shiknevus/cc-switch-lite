import { useMemo } from "react";
import {
  formatMoney,
  formatMoneyCompact,
  formatMoneyShort,
  formatMoneyTick,
  resolveMoneyDisplay,
  USD_DISPLAY,
} from "@/components/usage/format";
import { useSettingsQuery } from "@/lib/query";

/**
 * 用量金额的统一出口：用量面板、侧栏「今日」等费用都经这里显示。
 */
export function useMoneyFormat() {
  const { data: settings } = useSettingsQuery();
  const { currency, rate } = resolveMoneyDisplay(settings?.usageDisplay);

  return useMemo(() => {
    const display = { currency, rate };
    const converted = currency !== "USD";
    return {
      currency,
      rate,
      money: (usd: unknown, digits: number, fallback?: string) =>
        formatMoney(usd, digits, display, fallback),
      moneyCompact: (usd: number) => formatMoneyCompact(usd, display),
      moneyShort: (usd: unknown) => formatMoneyShort(usd, display),
      moneyTick: (value: unknown) => formatMoneyTick(value, display),
      usdHint: (usd: unknown, digits: number = 6) =>
        converted ? formatMoney(usd, digits, USD_DISPLAY) : undefined,
      moneyTitle: (usd: unknown, digits: number) =>
        converted
          ? `${formatMoney(usd, digits, display)} (${formatMoney(usd, digits, USD_DISPLAY)})`
          : formatMoney(usd, digits, display),
    };
  }, [currency, rate]);
}

