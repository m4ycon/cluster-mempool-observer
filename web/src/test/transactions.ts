import type { TransactionLookup, TransactionRef } from '../types/events';

/** A complete transaction row; override what a given test needs to be different. */
export function tx(
  txid: string,
  overrides: Partial<TransactionRef> = {},
): TransactionRef {
  return {
    txid,
    fee: 100,
    vsize: 200,
    first_seen_at: '2026-01-01T00:00:00.000Z',
    cluster_id: null,
    hollow: false,
    input_txids: [],
    ...overrides,
  };
}

/** A 200 carrying `lookup`, shaped like what `httpClient.get` unwraps. */
export function lookupResponse(lookup: TransactionLookup): Response {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: () => Promise.resolve(lookup),
  } as Response;
}

/** The txids a single `/transactions` request asked for. */
export function requestedTxids(url: string): string[] {
  const query = new URL(url, 'http://test').searchParams.get('txids') ?? '';
  return query.split(',').filter(Boolean);
}
