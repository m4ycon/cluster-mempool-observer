import type { GaugePoint } from '../types/generated/GaugePoint';
import type { SystemEvent } from '../types/generated/SystemEvent';
import type { SystemEventKind } from '../types/generated/SystemEventKind';
import dayjs from './dayjs';

/** Epoch-ms span the chart must not draw a connected line across. */
export interface DowntimeWindow {
  from: number;
  to: number;
}

/** Single source for which kinds mark a lifecycle transition -- widen here only. */
const LIFECYCLE_KINDS: SystemEventKind[] = ['server_started', 'server_stopped'];

/** Healthy neighbours sit ~1x resolution apart; past this a bucket is missing entirely. */
const GAP_FACTOR = 2;

function sortByCreatedAt(events: SystemEvent[]): SystemEvent[] {
  return [...events].sort(
    (a, b) => dayjs(a.created_at).valueOf() - dayjs(b.created_at).valueOf(),
  );
}

/** Pairs server_stopped -> next server_started into windows the chart must not draw across. */
export function downtimeWindows(
  events: SystemEvent[],
  domainTo: number,
): DowntimeWindow[] {
  const sorted = sortByCreatedAt(
    events.filter((e) => LIFECYCLE_KINDS.includes(e.kind)),
  );

  const windows: DowntimeWindow[] = [];
  let openFrom: number | null = null;
  for (const e of sorted) {
    const t = dayjs(e.created_at).valueOf();
    if (e.kind === 'server_stopped') {
      // A second stop with no start between: downtime began at the first one.
      if (openFrom === null) openFrom = t;
      // A start with nothing open is a first boot / fresh DB, not an outage.
    } else if (openFrom !== null) {
      if (t > openFrom) windows.push({ from: openFrom, to: t });
      openFrom = null;
    }
  }
  if (openFrom !== null && domainTo > openFrom) {
    windows.push({ from: openFrom, to: domainTo });
  }
  return windows;
}

/** The subset of events that get a dashed vertical marker, sorted. */
export function lifecycleMarkers(events: SystemEvent[]): SystemEvent[] {
  return sortByCreatedAt(
    events.filter((e) => LIFECYCLE_KINDS.includes(e.kind)),
  );
}

/** Splits an ascending series into runs safe to draw as connected lines. */
export function splitSegments(
  points: GaugePoint[],
  resolutionSecs: number,
  windows: DowntimeWindow[],
): GaugePoint[][] {
  if (points.length === 0) return [];
  const maxGapMs = GAP_FACTOR * resolutionSecs * 1000;

  const segments: GaugePoint[][] = [];
  let current: GaugePoint[] = [points[0]];
  let ta = dayjs(points[0].sampled_at).valueOf();
  for (let i = 1; i < points.length; i++) {
    const b = points[i];
    const tb = dayjs(b.sampled_at).valueOf();
    const crossesDowntime = windows.some((w) => w.from < tb && w.to > ta);
    // A crash never writes server_stopped, but the missing bucket still shows as a gap.
    const missingBucket = tb - ta > maxGapMs;
    if (crossesDowntime || missingBucket) {
      segments.push(current);
      current = [b];
    } else {
      current.push(b);
    }
    ta = tb;
  }
  segments.push(current);
  return segments;
}
