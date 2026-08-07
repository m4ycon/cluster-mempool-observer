import type { ServerEvent, WsSubject } from '../types/events';

/**
 * The payload type for a given subject, derived from the `ServerEvent`
 * discriminated union rather than hand-maintained. `PayloadFor<'cluster.delta'>`
 * resolves to `ClusterDeltaEvent`.
 */
export type PayloadFor<S extends WsSubject> = Extract<
  ServerEvent,
  { subject: S }
>['payload'];
