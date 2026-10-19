import { emit, emitTo, listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { UsageDashboard } from "@/components/usage/UsageDashboard";
import { FloatBar } from "@/components/float-bar/FloatBar";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "@/components/ui/button";
import { Monitor, Moon, Sun, Globe, PanelTop } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTheme } from "@/components/theme-provider";
import { syncLanguageFromSettings } from "@/i18n";
import { settingsApi } from "@/lib/api/settings";
import { toast } from "@/lib/toast";
import { extractErrorMessage } from "@/utils/errorUtils";
import { WindowControlsContext } from "@/components/shell/AppPageHeader";
import { WindowControls } from "@/components/shell/WindowControls";
import { syncModelsDevPricingOnStartup } from "@/lib/modelsDevAutoSync";

export default function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { settings, autoSaveSettings } = useSettings();

  const [isFloatWindow, setIsFloatWindow] = useState(() => {
    try {
      return getCurrentWindow().label === "float_bar";
    } catch {
      return false;
    }
  });

  useEffect(() => {
    try {
      const win = getCurrentWindow();
      if (win.label === "float_bar") {
        setIsFloatWindow(true);
        document.documentElement.classList.add("float-window");
        document.body.classList.add("float-window");
        void win.setShadow(false).catch((error) => {
          console.debug("Failed to disable float bar shadow:", error);
        });
        return;
      }
    } catch (e) {
      console.debug("Float window detection error", e);
    }

    void getCurrentWindow().setDecorations(false).catch((error) => {
      console.debug("Failed to hide the native title bar:", error);
    });

    // 静默启动（开机自启 + 设置里开了静默）时不要主动 show：窗口要一直留在托盘
    void invoke<boolean>("is_silent_startup")
      .then((silent) => {
        if (silent) return;
        const win = getCurrentWindow();
        return win.show().then(() => win.setFocus());
      })
      .catch((error) => {
        console.debug("Failed to show the main window:", error);
      });

    // 启动时在后台触发一次 models.dev 自动同步检查（若设置开启且超过6小时）
    void syncModelsDevPricingOnStartup().catch((error) => {
      console.debug("Failed to auto-sync models.dev pricing on startup:", error);
    });
  }, []);

  useEffect(() => {
    if (settings?.language) {
      syncLanguageFromSettings(settings.language);
      if (i18n.language !== settings.language) {
        void i18n.changeLanguage(settings.language);
      }
    }
  }, [settings?.language, i18n]);

  // 监听全通道语言变更事件（来自托盘、Rust 广播或其它窗口）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<{ language: string }>("app-language-changed", (event) => {
      const nextLang = event.payload?.language;
      if (nextLang && nextLang !== i18n.language) {
        syncLanguageFromSettings(nextLang);
        void i18n.changeLanguage(nextLang);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [i18n]);

  // 只在后端设置变化时套用主题
  useEffect(() => {
    const savedTheme = settings?.theme;
    if (
      savedTheme === "light" ||
      savedTheme === "dark" ||
      savedTheme === "system"
    ) {
      setTheme(savedTheme);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings?.theme]);

  // 如果当前是桌面悬浮条独立窗口，直接渲染纯净透明悬浮条组件
  if (isFloatWindow) {
    return (
      <div className="w-screen h-screen bg-transparent overflow-visible select-none">
        <FloatBar />
      </div>
    );
  }

  const applyAutoLaunch = async (enabled: boolean, silent: boolean) => {
    const previousEnabled = settings?.launchOnStartup ?? false;
    const previousSilent = settings?.silentStartup ?? false;
    try {
      await settingsApi.setAutoLaunch(enabled, silent);
    } catch (error) {
      toast.error(
        t("settings.autoLaunchFailed", {
          defaultValue: "设置开机自启失败：{{error}}",
          error: extractErrorMessage(error),
        }),
      );
      return;
    }
    const saved = await autoSaveSettings({
      launchOnStartup: enabled,
      silentStartup: silent,
    });
    if (!saved) {
      try {
        await settingsApi.setAutoLaunch(previousEnabled, previousSilent);
      } catch (rollbackError) {
        console.error(
          "Failed to roll back auto-launch after settings persistence failure",
          rollbackError,
        );
      }
    }
  };

  const handleLanguageChange = async (lang: string) => {
    if (lang === i18n.language) return;
    syncLanguageFromSettings(lang);
    await i18n.changeLanguage(lang);
    void autoSaveSettings({ language: lang });
    void emit("app-language-changed", { language: lang });
    void emitTo("float_bar", "app-language-changed", { language: lang });
  };

  const cycleTheme = () => {
    const nextTheme = theme === "dark" ? "light" : theme === "light" ? "system" : "dark";
    setTheme(nextTheme);
    void autoSaveSettings({ theme: nextTheme });
  };

  const toggleFloatBar = async () => {
    try {
      const next = await invoke<boolean>("toggle_float_bar");
      void autoSaveSettings({ floatBarEnabled: next });
    } catch (e) {
      console.debug("Toggle float bar error:", e);
    }
  };

  const extraActions = (
    <div className="flex items-center gap-1 ps-1 border-s border-border">
      {/* 桌面悬浮顶栏快捷开关 */}
      <Button
        variant="ghost"
        size="icon"
        className={`h-7 w-7 transition-colors ${
          settings?.floatBarEnabled
            ? "text-purple-400 hover:text-purple-300 bg-purple-500/10"
            : "text-fg-3 hover:text-fg"
        }`}
        onClick={toggleFloatBar}
        title={
          settings?.floatBarEnabled
            ? t("floatBar.disableHint", { defaultValue: "隐藏桌面悬浮顶栏" })
            : t("floatBar.enableHint", { defaultValue: "显示桌面悬浮顶栏" })
        }
      >
        <PanelTop className="h-4 w-4" />
      </Button>

      {/* 语言切换 */}
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-fg-3 hover:text-fg"
            title={t("settings.language", { defaultValue: "Language" })}
          >
            <Globe className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-32 bg-surface border-border">
          <DropdownMenuItem onClick={() => handleLanguageChange("zh")}>
            简体中文
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => handleLanguageChange("zh-TW")}>
            繁體中文
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => handleLanguageChange("en")}>
            English
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => handleLanguageChange("ja")}>
            日本語
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* 主题切换（深色 → 浅色 → 跟随系统） */}
      <Button
        variant="ghost"
        size="icon"
        className="h-7 w-7 text-fg-3 hover:text-fg"
        onClick={cycleTheme}
        title={
          theme === "dark"
            ? t("settings.themeSwitchLight", { defaultValue: "Switch to light" })
            : theme === "light"
              ? t("settings.themeSwitchSystem", { defaultValue: "Switch to system" })
              : t("settings.themeSwitchDark", { defaultValue: "Switch to dark" })
        }
      >
        {theme === "dark" ? (
          <Sun className="h-4 w-4" />
        ) : theme === "light" ? (
          <Monitor className="h-4 w-4" />
        ) : (
          <Moon className="h-4 w-4" />
        )}
      </Button>
    </div>
  );

  return (
    <WindowControlsContext.Provider value={<WindowControls />}>
      <div className="flex h-screen w-screen flex-col bg-bg text-fg select-text overflow-hidden">
        {/* 主体用量仪表盘 */}
        <div className="min-h-0 flex-1 overflow-y-auto overflow-x-auto">
          <UsageDashboard
            refreshIntervalMs={settings?.usageDashboardRefreshIntervalMs}
            onRefreshIntervalChange={(usageDashboardRefreshIntervalMs) => {
              return autoSaveSettings({ usageDashboardRefreshIntervalMs });
            }}
            sessionAutoSyncEnabled={settings?.sessionAutoSyncEnabled ?? true}
            settingsReady={settings != null}
            onSessionAutoSyncEnabledChange={(sessionAutoSyncEnabled) => {
              return autoSaveSettings({ sessionAutoSyncEnabled });
            }}
            minimizeToTrayOnClose={settings?.minimizeToTrayOnClose ?? true}
            onMinimizeToTrayOnCloseChange={(minimizeToTrayOnClose) => {
              return autoSaveSettings({ minimizeToTrayOnClose });
            }}
            launchOnStartup={settings?.launchOnStartup ?? false}
            onLaunchOnStartupChange={(launchOnStartup) => {
              void applyAutoLaunch(launchOnStartup, settings?.silentStartup ?? false);
            }}
            silentStartup={settings?.silentStartup ?? false}
            onSilentStartupChange={(silentStartup) => {
              if (settings?.launchOnStartup) {
                void applyAutoLaunch(true, silentStartup);
              } else {
                void autoSaveSettings({ silentStartup });
              }
            }}
            floatBarEnabled={settings?.floatBarEnabled ?? true}
            onFloatBarEnabledChange={async (next) => {
              try {
                await invoke("toggle_float_bar", { enabled: next });
              } catch (e) {
                console.debug("Float bar toggle error:", e);
              }
              return autoSaveSettings({ floatBarEnabled: next });
            }}
            floatBarAutoHide={settings?.floatBarAutoHide ?? true}
            onFloatBarAutoHideChange={(next) => {
              return autoSaveSettings({ floatBarAutoHide: next });
            }}
            usageDisplay={settings?.usageDisplay}
            onUsageDisplayChange={(usageDisplay) => {
              return autoSaveSettings({ usageDisplay });
            }}
            headerExtraActions={extraActions}
          />
        </div>
      </div>
    </WindowControlsContext.Provider>
  );
}