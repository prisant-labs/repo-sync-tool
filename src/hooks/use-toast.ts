import { createContext, useContext } from "react";

export type ToastKind = "ok" | "info" | "error";

/** Raise a transient notification. Provided by `ToastProvider`. */
export type ToastFn = (kind: ToastKind, title: string, message?: string) => void;

export const ToastContext = createContext<ToastFn>(() => {});

export function useToast(): ToastFn {
  return useContext(ToastContext);
}
