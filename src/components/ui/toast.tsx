import { useCallback, useRef, useState } from "react";
import type { ReactNode } from "react";
import { AlertTriangle, Check, Info, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { ToastContext, type ToastFn, type ToastKind } from "@/hooks/use-toast";

type ToastItem = { id: number; kind: ToastKind; title: string; message?: string };

const ICONS: Record<ToastKind, typeof Check> = {
  ok: Check,
  info: Info,
  error: AlertTriangle,
};

const TONE: Record<ToastKind, string> = {
  ok: "text-status-sync",
  info: "text-primary",
  error: "text-status-failed",
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const idRef = useRef(0);

  const dismiss = useCallback((id: number) => {
    setItems((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const push = useCallback<ToastFn>(
    (kind, title, message) => {
      const id = (idRef.current += 1);
      setItems((prev) => [...prev, { id, kind, title, message }]);
      window.setTimeout(() => dismiss(id), 4200);
    },
    [dismiss],
  );

  return (
    <ToastContext.Provider value={push}>
      {children}
      <div className="fixed right-4 bottom-4 z-[100] flex w-80 max-w-[90vw] flex-col gap-2">
        {items.map((t) => {
          const Icon = ICONS[t.kind];
          return (
            <div
              key={t.id}
              className="flex items-start gap-3 rounded-md border border-border bg-popover p-3 shadow-lg"
            >
              <Icon className={cn("mt-0.5 size-4 shrink-0", TONE[t.kind])} />
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold">{t.title}</div>
                {t.message && <div className="mt-0.5 text-xs text-muted-foreground">{t.message}</div>}
              </div>
              <button
                onClick={() => dismiss(t.id)}
                className="text-muted-foreground hover:text-foreground"
                aria-label="Dismiss"
              >
                <X className="size-4" />
              </button>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
}
