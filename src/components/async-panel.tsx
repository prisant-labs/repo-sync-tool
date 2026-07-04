import type { ReactNode } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import type { AsyncState } from "@/hooks/use-async";
import { IpcError } from "@/lib/ipc";

/**
 * Renders the loading / error / empty / data states of a `useAsync` result so
 * screens only describe the happy path. Error state surfaces the backend's own
 * remediation text when the failure came across the IPC boundary.
 */
export function AsyncPanel<T>({
  state,
  children,
  emptyWhen,
  emptyMessage = "Nothing here yet.",
}: {
  state: AsyncState<T>;
  children: (data: NonNullable<T>) => ReactNode;
  emptyWhen?: (data: NonNullable<T>) => boolean;
  emptyMessage?: ReactNode;
}) {
  if (state.loading && state.data === null) {
    return (
      <div className="flex items-center justify-center gap-2 py-16 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" /> Loading...
      </div>
    );
  }

  if (state.error) {
    const remediation =
      state.error instanceof IpcError
        ? state.error.remediation
        : "Is the backend running? These screens read live data through the Tauri shell.";
    return (
      <div className="flex flex-col items-center gap-2 py-16 text-center">
        <AlertTriangle className="size-6 text-status-failed" />
        <div className="text-sm font-medium">{state.error.message}</div>
        <div className="max-w-sm text-xs text-muted-foreground">{remediation}</div>
      </div>
    );
  }

  // The null case is handled above; the cast tells TS the generic is non-null here.
  if (state.data === null || (emptyWhen && emptyWhen(state.data as NonNullable<T>))) {
    return <div className="py-2 text-center text-sm text-muted-foreground">{emptyMessage}</div>;
  }

  return <>{children(state.data as NonNullable<T>)}</>;
}
