import type { AppErrorPayload } from "@/lib/bindings";

/** A backend error surfaced across the typed IPC boundary. */
export class IpcError extends Error {
  readonly code: string;
  readonly remediation: string;
  readonly payload: AppErrorPayload;

  constructor(payload: AppErrorPayload) {
    super(payload.message);
    this.name = "IpcError";
    this.code = payload.code;
    this.remediation = payload.remediation;
    this.payload = payload;
  }
}

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: AppErrorPayload };

/**
 * Collapse a tauri-specta `Result` into a throw-on-error promise, so the data
 * hooks can treat every command uniformly (data on success, `IpcError` on
 * failure). The generated commands never reject; they resolve to this union.
 */
export async function unwrap<T>(promise: Promise<Result<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new IpcError(result.error);
  return result.data;
}
