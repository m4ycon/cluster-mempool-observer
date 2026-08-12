import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { HttpClient, HttpError } from './http';

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    statusText: ok ? 'OK' : 'Error',
    json: () => Promise.resolve(body),
  } as Response;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('HttpClient.get', () => {
  it('returns parsed JSON on a successful GET', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse({ hello: 'world' })),
    );
    const client = new HttpClient('https://api.test');

    const result = await client.get<{ hello: string }>('/things');

    expect(result).toEqual({ hello: 'world' });
    expect(fetch).toHaveBeenCalledWith(
      'https://api.test/things',
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
  });

  it('rejects with HttpError carrying the status on a 400', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 400)),
    );
    const client = new HttpClient('https://api.test');

    await expect(client.get('/things')).rejects.toBeInstanceOf(HttpError);
    await expect(client.get('/things')).rejects.toMatchObject(
      expect.objectContaining({ name: 'HttpError', status: 400 }),
    );
  });

  it('rejects with HttpError carrying the status on a 500', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(null, false, 500)),
    );
    const client = new HttpClient('https://api.test');

    await expect(client.get('/things')).rejects.toMatchObject(
      expect.objectContaining({ name: 'HttpError', status: 500 }),
    );
  });

  it('aborts the request when the caller-supplied signal aborts', async () => {
    const controller = new AbortController();
    vi.stubGlobal(
      'fetch',
      vi.fn((_url: string, init?: RequestInit) => {
        return new Promise((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            reject(new DOMException('aborted', 'AbortError'));
          });
        });
      }),
    );
    const client = new HttpClient('https://api.test');

    const promise = client.get('/things', { signal: controller.signal });
    controller.abort();

    await expect(promise).rejects.toThrow();
  });

  describe('timeout', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('rejects once the timeout elapses on a request that never settles', async () => {
      vi.stubGlobal(
        'fetch',
        vi.fn((_url: string, init?: RequestInit) => {
          return new Promise((_resolve, reject) => {
            init?.signal?.addEventListener('abort', () => {
              reject(new DOMException('aborted', 'TimeoutError'));
            });
          });
        }),
      );
      const client = new HttpClient('https://api.test', 1_000);

      const promise = client.get('/things');
      const assertion = expect(promise).rejects.toThrow();
      await vi.advanceTimersByTimeAsync(1_000);

      await assertion;
    });
  });
});
