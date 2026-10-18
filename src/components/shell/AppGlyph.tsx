import { Monitor, Terminal } from "lucide-react";
import type { AppId } from "@/lib/api";
import { ProviderIcon } from "@/components/ProviderIcon";
import { cn } from "@/lib/utils";

export const APP_DISPLAY_NAME: Record<AppId, string> = {
  claude: "Claude Code",
  "claude-desktop": "Claude Desktop",
  codex: "Codex",
  gemini: "Antigravity CLI",
  grokbuild: "Grok Build",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
  pi: "Pi",
  mcode: "MiniMax Code",
  dsh: "DeepSeek Shell",
};

const APP_ICON_NAME: Record<AppId, string> = {
  claude: "claude",
  "claude-desktop": "claude",
  codex: "openai",
  gemini: "gemini",
  grokbuild: "grok",
  opencode: "opencode",
  openclaw: "openclaw",
  hermes: "hermes",
  pi: "pi",
  mcode: "minimax",
  dsh: "deepseek",
};

const APP_BADGE_ICON: Partial<Record<AppId, typeof Terminal>> = {
  claude: Terminal,
  "claude-desktop": Monitor,
};

interface AppGlyphProps {
  app: AppId;
  size?: number;
  badgeClassName?: string;
  className?: string;
}

export function AppGlyph({
  app,
  size = 16,
  badgeClassName,
  className,
}: AppGlyphProps) {
  const BadgeIcon = APP_BADGE_ICON[app];
  return (
    <span
      aria-hidden="true"
      className={cn(
        "relative inline-flex shrink-0 items-center justify-center",
        className,
      )}
      style={{ width: size + 2, height: size + 2 }}
    >
      <ProviderIcon
        icon={APP_ICON_NAME[app]}
        name=""
        size={size}
        showFallback={false}
      />
      {BadgeIcon && (
        <span
          className={cn(
            "absolute -bottom-1 -right-1 flex h-[11px] w-[11px] items-center justify-center rounded-[3px] bg-sidebar text-fg-2",
            badgeClassName,
          )}
        >
          <BadgeIcon className="h-[9px] w-[9px]" strokeWidth={3} />
        </span>
      )}
    </span>
  );
}
