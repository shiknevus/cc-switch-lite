import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import "./i18n";
import { QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { queryClient } from "@/lib/query";
import { Toaster } from "@/components/ui/sonner";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { installGlobalErrorHandlers } from "@/lib/frontendLogger";
import { initializeInputModality } from "@/lib/inputModality";
import { initializeWindowActivity } from "@/lib/windowActivity";

// 全局副作用只需要挂一次：前端错误日志（脱敏）落盘、窗口活跃心跳、
// 以及键盘/指针输入模态（决定焦点框与 tooltip 是否显示）。
installGlobalErrorHandlers();
initializeInputModality();
initializeWindowActivity();

try {
  const ua = navigator.userAgent || "";
  const plat = (navigator.platform || "").toLowerCase();
  const isMac = /mac/i.test(ua) || plat.includes("mac");
  if (isMac) {
    document.body.classList.add("is-mac");
  }
} catch {
  // ignore
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider defaultTheme="system" storageKey="cc-switch-lite-theme">
        <TooltipProvider delayDuration={0} skipDelayDuration={0}>
          <ErrorBoundary>
            <App />
          </ErrorBoundary>
        </TooltipProvider>
        <Toaster />
      </ThemeProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);

