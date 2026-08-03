import { vi } from "vitest";
import type { commands } from "@/lib/bindings";

/**
 * Mock IPC for frontend tests.
 *
 * Everything in `src/` reaches the backend through the generated `commands`
 * object in `@/lib/bindings`, never through raw `invoke`. That single seam is
 * what makes the frontend testable at all: stub `commands` and the whole UI runs
 * with no Tauri runtime, no WebView, and no SQLite.
 *
 * Two properties are deliberate.
 *
 * First, results are built with [`ok`] and [`err`] rather than hand-written
 * object literals, so a test cannot accidentally assert against a result shape
 * the real bindings never produce.
 *
 * Second, the stubs are TYPED against the real `commands`. If a command's
 * signature changes, a test that mocks it stops compiling. That is the point: a
 * mock that silently drifts from the contract is worse than no mock, because it
 * keeps passing while the real call has changed underneath it.
 */

/** The generated bindings' success shape. */
export function ok<T>(data: T) {
  return { status: "ok", data } as const;
}

/**
 * The generated bindings' error shape. `code` is the stable `AppError` code
 * (for example "fetch_failed"); the UI branches on it, so tests should too
 * rather than matching on message text.
 */
export function err(code: string, message = code) {
  return {
    status: "error",
    error: { code, message, context: null },
  } as const;
}

/** Any key on the generated `commands` object. */
export type CommandName = keyof typeof commands;

/**
 * Replace one generated command for the duration of a test.
 *
 * Uses `vi.spyOn`, so `vi.restoreAllMocks()` (or `restoreMocks: true`) puts the
 * real binding back. Prefer this over reassigning the import, which leaks across
 * test files and produces failures that depend on file order.
 *
 * ```ts
 * import { commands } from "@/lib/bindings";
 * mockCommand(commands, "repoList", async () => ok([]));
 * ```
 */
export function mockCommand<K extends CommandName>(
  target: typeof commands,
  name: K,
  impl: (typeof commands)[K],
) {
  return vi.spyOn(target, name).mockImplementation(impl as never);
}
