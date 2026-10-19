import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportFrontendError } from "@/lib/frontendLogger";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  public override state: State = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public override componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    reportFrontendError("ErrorBoundary", error, errorInfo.componentStack ?? undefined);
  }

  private handleReload = () => {
    window.location.reload();
  };

  public override render() {
    if (this.state.hasError) {
      return (
        <div className="flex h-screen w-screen flex-col items-center justify-center gap-4 bg-background p-6 text-foreground">
          <div className="flex max-w-md flex-col items-center gap-3 text-center">
            <h2 className="text-lg font-semibold">页面遇到错误</h2>
            <p className="text-sm text-muted-foreground break-all">
              {this.state.error?.message || "应用程序渲染异常，请尝试重新加载。"}
            </p>
            <div className="mt-2 flex gap-3">
              <button
                type="button"
                onClick={this.handleReload}
                className="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground shadow transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                重新加载
              </button>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
