import { useCallback, useEffect, useState } from "react";

export type AsyncState<T> = {
  data: T | null;
  error: Error | null;
  loading: boolean;
};

/**
 * Minimal query hook: runs `fn` on mount and whenever `deps` change, exposing
 * loading / error / data plus a manual `refetch`.
 *
 * A deliberate, dependency-free stand-in for React Query at this stage. When
 * caching, deduping, or background refetch become worth a dependency, this is
 * the single place to swap; every screen consumes it through the typed hooks in
 * `hooks/queries.ts`, not directly.
 */
export function useAsync<T>(
  fn: () => Promise<T>,
  deps: ReadonlyArray<unknown>,
): AsyncState<T> & { refetch: () => void } {
  const [state, setState] = useState<AsyncState<T>>({
    data: null,
    error: null,
    loading: true,
  });
  const [nonce, setNonce] = useState(0);
  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    // No synchronous setState here: initial state starts loading, and on a
    // refetch we intentionally keep the previous data visible until the next
    // result lands (the panel only shows the loader when data is still null).
    let active = true;
    fn().then(
      (data) => {
        if (active) setState({ data, error: null, loading: false });
      },
      (err: unknown) => {
        if (active) {
          setState({
            data: null,
            error: err instanceof Error ? err : new Error(String(err)),
            loading: false,
          });
        }
      },
    );
    return () => {
      active = false;
    };
    // fn identity is intentionally excluded; re-runs are gated on deps + nonce.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce]);

  return { ...state, refetch };
}
