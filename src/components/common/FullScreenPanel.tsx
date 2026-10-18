import React, { useEffect } from "react";
import { createPortal } from "react-dom";
import { ChevronLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { cn } from "@/lib/utils";

interface FullScreenPanelProps {
  isOpen: boolean;
  title: string;
  subtitle?: React.ReactNode;
  backLabel?: string;
  actions?: React.ReactNode;
  onClose: () => void;
  children: React.ReactNode;
  footer?: React.ReactNode;
  trackUnsavedChanges?: boolean;
  className?: string;
}

export function FullScreenPanel({
  isOpen,
  title,
  subtitle,
  backLabel,
  actions,
  onClose,
  children,
  footer,
  className,
}: FullScreenPanelProps) {
  const { t } = useTranslation();

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      className={cn(
        "fixed inset-0 z-50 flex flex-col bg-bg-app text-fg-1",
        className,
      )}
    >
      <AppPageHeader
        leading={
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={onClose}
            aria-label={backLabel ?? t("common.back", "返回")}
          >
            <ChevronLeft className="h-5 w-5" />
          </Button>
        }
        title={title}
        subtitle={subtitle}
        actions={actions}
      />
      <div className="flex-1 overflow-y-auto px-6 py-4">{children}</div>
      {footer && (
        <div className="border-t border-border px-6 py-3 bg-subtle">
          {footer}
        </div>
      )}
    </div>,
    document.body,
  );
}
