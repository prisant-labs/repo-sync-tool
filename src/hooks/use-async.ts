import { useCallback, useEffect, useRef, useState } from "react";

export type AsyncState<T> = {
  data: T | null;
  error: Error | null;
  loading: boolean;
};

export type AsyncOptions = {
  /**
   * Drop the previous result the moment `deps` change, instead of showing it
   * until the next one lands.
   *
   * OFF by default, because for most consumers keeping the old data is the right
   * behavior: the Repos name filter re-runs on every keystroke, and blanking the
   * list between characters would flicker badly while showing nothing the user
   * did not already have.
   *
   * Turn it ON when a dependency change makes the previous result WRONG rather
   * than merely stale. The Activity screen is the case: its deps are the filter
   * itself, so holding the old rows renders the previous filter's results
   * underneath the newly active chips. Selecting "Failed" would briefly list
   * successful entries, and clearing a filter that matched nothing would briefly
   * claim "No activity yet". For a view whose entire job is to be an accurate
   * audit trail, a moment of showing the wrong rows is worse than a moment of
   * showing none.
   *
   * A manual `refetch()` is deliberately NOT affected: re-running the same query
   * is the flicker-free case this exists to preserve.
   */
  clearDataOnDepsChange?: boolean;
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
  options: AsyncOptions = {},
): AsyncState<T> & { refetch: () => void } {
  const [state, setState] = useState<AsyncState<T>>({
    data: null,
    error: null,
    loading: true,
  });
  const [nonce, setNonce] = useState(0);
  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  // The deps as they were on the previous run, so the effect can tell a DEPS
  // change from a `refetch()`. Both re-run the effect and only the first should
  // discard the previous result. Starts undefined so the very first run counts
  // as a change, which is harmless because `data` is already null there.
  const previousDeps = useRef<string | undefined>(undefined);
  const { clearDataOnDepsChange = false } = options;

  useEffect(() => {
    const depsKey = JSON.stringify(deps);
    const depsChanged = previousDeps.current !== depsKey;
    previousDeps.current = depsKey;

    // On a refetch we intentionally keep the previous data visible until the
    // next result lands (the panel only shows the loader when data is still
    // null). An opted-in consumer additionally discards it when the DEPS moved,
    // because there the old result answers a question nobody asked any more.
    if (clearDataOnDepsChange && depsChanged) {
      setState({ data: null, error: null, loading: true });
    }
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
