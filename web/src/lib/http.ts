const DEFAULT_TIMEOUT_MS = 10_000;

export interface HttpClientGetOptions {
  signal?: AbortSignal;
}

/** Thrown on a non-OK response; carries the status so callers can distinguish 4xx from 5xx. */
export class HttpError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'HttpError';
    this.status = status;
  }
}

/** Thin fetch wrapper centralizing base URL, timeout and cancellation. */
export class HttpClient {
  private readonly baseUrl: string;
  private readonly timeoutMs: number;

  constructor(baseUrl: string, timeoutMs = DEFAULT_TIMEOUT_MS) {
    this.baseUrl = baseUrl;
    this.timeoutMs = timeoutMs;
  }

  async get<T>(path: string, opts?: HttpClientGetOptions): Promise<T> {
    const timeoutSignal = AbortSignal.timeout(this.timeoutMs);
    const signal = opts?.signal
      ? AbortSignal.any([opts.signal, timeoutSignal])
      : timeoutSignal;

    const res = await fetch(`${this.baseUrl}${path}`, { signal });
    if (!res.ok) {
      throw new HttpError(
        res.status,
        `GET ${path} failed: ${res.status} ${res.statusText}`,
      );
    }
    return (await res.json()) as T;
  }
}

export const httpClient = new HttpClient(import.meta.env.VITE_API_BASE_URL);
