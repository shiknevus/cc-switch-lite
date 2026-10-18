import { useTranslation } from "react-i18next";
import { PanelTop } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { HelpTip } from "@/components/ui/help-tip";

interface WindowBehaviorSectionProps {
  /** 设置还没读出来时不下发默认真值，避免把没读过当成用户选了「开」 */
  value: boolean;
  onChange: (next: boolean) => void;
}

/**
 * 用量「设置」页签里的「窗口行为」：关窗是隐到托盘常驻后台，还是直接退出进程。
 *
 * 这一项决定后端 on_window_event 里 CloseRequested 走哪条分支：开着的时候关窗只是
 * hide + 收掉任务栏图标，进程、托盘和后台定时同步都继续跑。
 */
export function WindowBehaviorSection({
  value,
  onChange,
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
      <div className="mt-2 flex items-center justify-between gap-4 rounded-panel bg-subtle px-4 py-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-body text-fg-1">
            {t("settings.minimizeToTray")}
          </span>
          <HelpTip title={t("settings.minimizeToTray")}>
            {t("settings.minimizeToTrayDescription")}
          </HelpTip>
        </div>
        <Switch
          checked={value}
          onCheckedChange={onChange}
          aria-label={t("settings.minimizeToTray")}
        />
      </div>
      <p className="m-0 mt-2 flex items-start gap-1.5 text-caption text-fg-3">
        <PanelTop aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        {t("usage.windowBehavior.trayHint")}
      </p>
    </section>
  );
}
