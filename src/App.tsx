import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { UsageDashboard } from "@/components/usage/UsageDashboard";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "@/components/ui/button";
import { Monitor, Moon, Sun, Globe } from "lucide-react";
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
import { isMac } from "@/lib/platform";
import { syncModelsDevPricingOnStartup } from "@/lib/modelsDevAutoSync";

export default function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { settings, autoSaveSettings } = useSettings();

  useEffect(() => {
    void getCurrentWindow().setDecorations(false).catch((error) => {
      console.debug("Failed to hide the native title bar:", error);
    });
    // 静默启动（开机自启 + 设置里开了静默）时不要主动 show：窗口要一直留在托盘，
    // 否则前端一渲染完就自己弹出来，静默设置等于没生效。后端启动时已经决定好了。
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
    syncLanguageFromSettings(settings?.language);
  }, [settings?.language]);

  // 只在后端设置变化时套用主题。setTheme 的身份会跟着当前主题变，
  // 放进依赖会在保存写回前用旧设置把刚切换的主题盖回去。
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

  /**
   * 开关开机自启：先落系统（注册表 / 登录项），成了再存设置。
   *
   * 顺序不能反：系统登记可能失败（组策略、macOS 拒绝授权），先把设置写成「开」
   * 而系统里没登记，界面就会显示已开启但重启后并不自启。失败时保持原状并提示。
   */
  const applyAutoLaunch = async (enabled: boolean, silent: boolean) => {
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
    await autoSaveSettings({ launchOnStartup: enabled, silentStartup: silent });
  };

  const handleLanguageChange = (lang: string) => {
    i18n.changeLanguage(lang);
    void autoSaveSettings({ language: lang });
  };

  /**
   * 主题三态循环：深色 → 浅色 → 跟随系统 → 深色……
   * 只保留两态会让用户手动切过一次后就再也回不到「跟随系统」，
   * 与 README 宣称的 system/light/dark 三态不符。
   */
  const cycleTheme = () => {
    const nextTheme = theme === "dark" ? "light" : theme === "light" ? "system" : "dark";
    setTheme(nextTheme);
    void autoSaveSettings({ theme: nextTheme });
  };

  const extraActions = (
    <div className="flex items-center gap-1 ps-1 border-s border-border">
      {/* 语言切换 */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-fg-3 hover:text-fg"
            title="Language"
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
            ? "Switch to light"
            : theme === "light"
              ? "Switch to system"
              : "Switch to dark"
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
    <WindowControlsContext.Provider value={isMac() ? null : <WindowControls />}>
      <div className="flex h-screen w-screen flex-col bg-bg text-fg select-none overflow-hidden">
      {/* 主体用量仪表盘 */}
      <div className="min-h-0 flex-1 overflow-y-auto overflow-x-auto">
        <UsageDashboard
          refreshIntervalMs={settings?.usageDashboardRefreshIntervalMs}
          onRefreshIntervalChange={(usageDashboardRefreshIntervalMs) => {
            void autoSaveSettings({ usageDashboardRefreshIntervalMs });
          }}
          sessionAutoSyncEnabled={settings?.sessionAutoSyncEnabled ?? true}
          settingsReady={settings != null}
          onSessionAutoSyncEnabledChange={(sessionAutoSyncEnabled) => {
            void autoSaveSettings({ sessionAutoSyncEnabled });
          }}
          minimizeToTrayOnClose={settings?.minimizeToTrayOnClose ?? true}
          onMinimizeToTrayOnCloseChange={(minimizeToTrayOnClose) => {
            void autoSaveSettings({ minimizeToTrayOnClose });
          }}
          launchOnStartup={settings?.launchOnStartup ?? false}
          onLaunchOnStartupChange={(launchOnStartup) => {
            // 静默启动靠启动项里的 --silent 参数生效，所以开关自启时把当前静默
            // 设置一起带下去，别让启动项里的参数和界面上的选择对不上。
            void applyAutoLaunch(launchOnStartup, settings?.silentStartup ?? false);
          }}
          silentStartup={settings?.silentStartup ?? false}
          onSilentStartupChange={(silentStartup) => {
            // 静默只对「已开启自启」有意义；自启没开时只存设置，不动系统。
            if (settings?.launchOnStartup) {
              void applyAutoLaunch(true, silentStartup);
            } else {
              void autoSaveSettings({ silentStartup });
            }
          }}
          usageDisplay={settings?.usageDisplay}
          onUsageDisplayChange={(usageDisplay) => {
            void autoSaveSettings({ usageDisplay });
          }}
          headerExtraActions={extraActions}
        />
      </div>
      </div>
    </WindowControlsContext.Provider>
  );
}
