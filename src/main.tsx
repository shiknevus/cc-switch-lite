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
          <App />
        </TooltipProvider>
        <Toaster />
      </ThemeProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);

