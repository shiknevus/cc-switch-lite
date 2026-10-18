import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import en from "./locales/en.json";
import zh from "./locales/zh.json";
import zhTW from "./locales/zh-TW.json";
import ja from "./locales/ja.json";

export type Language = "zh" | "en" | "zh-TW" | "ja";

const DEFAULT_LANGUAGE: Language = "zh";

const isLanguage = (value: unknown): value is Language =>
  value === "zh" || value === "en" || value === "zh-TW" || value === "ja";

const getInitialLanguage = (): Language => {
  if (typeof window !== "undefined") {
    try {
      const stored = window.localStorage.getItem("language");
      if (isLanguage(stored)) {
        return stored;
      }
    } catch (error) {
      console.warn("[i18n] Failed to read stored language preference", error);
    }
  }

  const navigatorLang =
    typeof navigator !== "undefined"
      ? (navigator.language?.toLowerCase() ??
        navigator.languages?.[0]?.toLowerCase())
      : undefined;

  if (navigatorLang === "zh-tw" || navigatorLang === "zh-hk") {
    return "zh-TW";
  }

  if (navigatorLang?.startsWith("zh")) {
    return "zh";
  }

  if (navigatorLang?.startsWith("ja")) {
    return "ja";
  }

  if (navigatorLang?.startsWith("en")) {
    return "en";
  }

  return navigatorLang ? "en" : DEFAULT_LANGUAGE;
};

const resources = {
  en: {
    translation: en,
  },
  zh: {
    translation: zh,
  },
  "zh-TW": {
    translation: zhTW,
  },
  ja: {
    translation: ja,
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: getInitialLanguage(),
  fallbackLng: "en",
  interpolation: {
    escapeValue: false,
  },
  debug: false,
});

export const syncLanguageFromSettings = (language?: string | null): void => {
  if (!isLanguage(language)) return;

  if (i18n.language !== language) {
    void i18n.changeLanguage(language);
  }

  // 页面语言声明与 UI 语言保持一致：切到英文后 <html lang> 不应仍写 zh-CN。
  if (typeof document !== "undefined") {
    document.documentElement.lang = language;
  }

  try {
    if (window.localStorage.getItem("language") !== language) {
      window.localStorage.setItem("language", language);
    }
  } catch (error) {
    console.warn("[i18n] Failed to persist language preference", error);
  }
};

// i18next 语言变化事件同样同步 <html lang>（菜单直接调 changeLanguage 的路径）。
i18n.on("languageChanged", (language) => {
  if (typeof document !== "undefined") {
    document.documentElement.lang = language;
  }
});

export default i18n;