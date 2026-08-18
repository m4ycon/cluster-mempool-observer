import { useEffect, useState } from 'react';
import { httpClient } from '../lib/http';

export type HttpGetState<T> =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'loaded'; data: T };

/** Fetches `path` on mount and whenever it changes; loading/error/loaded stay distinguishable. */
export function useHttpGet<T>(path: string): HttpGetState<T> {
  const [state, setState] = useState<HttpGetState<T>>({ status: 'loading' });

  useEffect(() => {
    const controller = new AbortController();
    // A refetch (path changed) keeps the previous data on screen; only a cold start blanks.
    setState((prev) =>
      prev.status === 'loaded' ? prev : { status: 'loading' },
    );

    httpClient
      .get<T>(path, { signal: controller.signal })
      .then((data) => {
        setState({ status: 'loaded', data });
      })
      .catch((err: unknown) => {
        // controller.signal only aborts via the cleanup below (unmount/path change).
        // A timeout aborts httpClient's own combined signal instead, leaving this one
        // clear, so it falls through and correctly surfaces as an error.
        if (controller.signal.aborted) return;
        setState({
          status: 'error',
          error: err instanceof Error ? err : new Error(String(err)),
        });
      });

    return () => {
      controller.abort();
    };
  }, [path]);

  return state;
}
