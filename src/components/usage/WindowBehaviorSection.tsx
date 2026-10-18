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
 * 用量「设置」页签里的「窗口行为」：关窗是否隐到托盘常驻、以及开机自启 + 静默启动。
 *
 * 关窗开关决定后端 on_window_event 里 CloseRequested 走哪条分支：开着的时候关窗
 * 只是 hide + 收掉任务栏图标，进程、托盘和后台定时同步都继续跑。
 *
 * 静默启动依赖开机自启才成立（靠启动项里的 --silent 参数区分「开机拉起来」和
 * 「用户手动双击」），所以自启关掉时把它一并置灰。
 */
export function WindowBehaviorSection({
  value,
  onChange,
  launchOnStartup,
  onLaunchOnStartupChange,
  silentStartup,
  onSilentStartupChange,
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
      </div>
      <p className="m-0 mt-2 flex items-start gap-1.5 text-caption text-fg-3">
        <PanelTop aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        {t("usage.windowBehavior.trayHint")}
      </p>
      <p className="m-0 mt-1.5 flex items-start gap-1.5 text-caption text-fg-3">
        <Power aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        {t("usage.windowBehavior.autoLaunchHint")}
      </p>
    </section>
  );
}
