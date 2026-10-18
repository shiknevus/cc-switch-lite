import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { UsageDashboard } from "@/components/usage/UsageDashboard";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "@/components/ui/button";
import { Moon, Sun, Globe } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTheme } from "@/components/theme-provider";
import { syncLanguageFromSettings } from "@/i18n";
import { WindowControlsContext } from "@/components/shell/AppPageHeader";
import { WindowControls } from "@/components/shell/WindowControls";

export default function App() {
  const { i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { settings, autoSaveSettings } = useSettings();

  useEffect(() => {
    void getCurrentWindow().setDecorations(false).catch((error) => {
      console.debug("Failed to hide the native title bar:", error);
    });
  }, []);

  useEffect(() => {
    syncLanguageFromSettings(settings?.language);
    const savedTheme = settings?.theme;
    if (
      savedTheme === "light" ||
      savedTheme === "dark" ||
      savedTheme === "system"
    ) {
      if (savedTheme !== theme) setTheme(savedTheme);
    }
  }, [settings?.language, settings?.theme, setTheme, theme]);

  const handleLanguageChange = (lang: string) => {
    i18n.changeLanguage(lang);
    void autoSaveSettings({ language: lang });
  };

  const toggleTheme = () => {
    const nextTheme = theme === "dark" ? "light" : "dark";
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
          <DropdownMenuItem onClick={() => handleLanguageChange("en")}>
            English
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* 主题切换 */}
      <Button
        variant="ghost"
        size="icon"
        className="h-7 w-7 text-fg-3 hover:text-fg"
        onClick={toggleTheme}
        title={theme === "dark" ? "Switch to light" : "Switch to dark"}
      >
        {theme === "dark" ? (
          <Sun className="h-4 w-4" />
        ) : (
          <Moon className="h-4 w-4" />
        )}
      </Button>
    </div>
  );

  return (
    <WindowControlsContext.Provider value={<WindowControls />}>
      <div className="flex h-screen w-screen flex-col bg-bg text-fg select-none overflow-hidden">
      {/* 主体用量仪表盘 */}
      <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
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
