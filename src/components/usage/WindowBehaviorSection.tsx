import { useTranslation } from "react-i18next";
import { PanelTop, Power } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { HelpTip } from "@/components/ui/help-tip";

interface WindowBehaviorSectionProps {
  /** 设置还没读出来时不下发默认真值，避免把没读过当成用户选了「开」 */
  value: boolean;
  onChange: (next: boolean) => void;
  launchOnStartup: boolean;
  onLaunchOnStartupChange: (next: boolean) => void;
  silentStartup: boolean;
  onSilentStartupChange: (next: boolean) => void;
  floatBarEnabled?: boolean;
  onFloatBarEnabledChange?: (next: boolean) => void;
  floatBarAutoHide?: boolean;
  onFloatBarAutoHideChange?: (next: boolean) => void;
}

/** 一行「标签 + ? + 开关」，三个开关共用同一套排版。 */
function ToggleRow({
  label,
  help,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  help: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-control px-3 py-2 hover:bg-surface">
      <div className="flex min-w-0 items-center gap-2">
        <span className={disabled ? "text-body text-fg-3" : "text-body text-fg-1"}>
          {label}
        </span>
        <HelpTip title={label}>{help}</HelpTip>
      </div>
      <Switch
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
        aria-label={label}
      />
    </div>
  );
}

/**
 * 用量「设置」页签里的「窗口行为」：关窗是否隐到托盘常驻、开机自启 + 静默启动、以及桌面悬浮顶栏。
 */
export function WindowBehaviorSection({
  value,
  onChange,
  launchOnStartup,
  onLaunchOnStartupChange,
  silentStartup,
  onSilentStartupChange,
  floatBarEnabled = true,
  onFloatBarEnabledChange,
  floatBarAutoHide = true,
  onFloatBarAutoHideChange,
}: WindowBehaviorSectionProps) {
  const { t } = useTranslation();

  return (
    <section aria-labelledby="usage-window-behavior-title">
      <h3
        id="usage-window-behavior-title"
        className="m-0 text-strong font-semibold text-fg-1"
      >
        {t("settings.windowBehavior")}
      </h3>
      <div className="mt-2 flex flex-col rounded-panel bg-subtle px-1 py-1.5">
        <ToggleRow
          label={t("settings.minimizeToTray")}
          help={t("settings.minimizeToTrayDescription")}
          checked={value}
          onChange={onChange}
        />
        <ToggleRow
          label={t("settings.launchOnStartup")}
          help={t("settings.launchOnStartupDescription")}
          checked={launchOnStartup}
          onChange={onLaunchOnStartupChange}
        />
        <ToggleRow
          label={t("settings.silentStartup")}
          help={t("settings.silentStartupDescription")}
          checked={silentStartup}
          onChange={onSilentStartupChange}
          disabled={!launchOnStartup}
        />
        <ToggleRow
          label={t("settings.floatBarEnabled", { defaultValue: "桌面悬浮顶栏" })}
          help={t("settings.floatBarEnabledDescription", {
            defaultValue: "在屏幕边缘常驻极简用量条，随时掌握 Token 和费用开销",
          })}
          checked={floatBarEnabled}
          onChange={onFloatBarEnabledChange ?? (() => {})}
        />
        <ToggleRow
          label={t("settings.floatBarAutoHide", { defaultValue: "贴边自动隐藏" })}
          help={t("settings.floatBarAutoHideDescription", {
            defaultValue: "鼠标离开时自动收起为微光指示条，悬停时平滑弹出单行用量",
          })}
          checked={floatBarAutoHide}
          onChange={onFloatBarAutoHideChange ?? (() => {})}
          disabled={!floatBarEnabled}
        />
      </div>
      <p className="m-0 mt-2 flex items-start gap-1.5 text-caption text-fg-3">
        <PanelTop aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        {t("usage.windowBehavior.trayHint")}
      </p>
      <p className="m-0 mt-1.5 flex items-start gap-1.5 text-caption text-fg-3">
        <PanelTop aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0 text-purple-400" />
        {t("floatBar.hint", {
          defaultValue: "悬浮顶栏可任意拖动，点击托盘图标或顶部图标可快速切换显示/隐藏。",
        })}
      </p>
      <p className="m-0 mt-1.5 flex items-start gap-1.5 text-caption text-fg-3">
        <Power aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        {t("usage.windowBehavior.autoLaunchHint")}
      </p>
    </section>
  );
}