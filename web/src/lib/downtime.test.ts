import { describe, expect, it } from 'vitest';
import type { MempoolMetricPoint } from '../types/generated/MempoolMetricPoint';
import type { SystemEvent } from '../types/generated/SystemEvent';
import type { SystemEventKind } from '../types/generated/SystemEventKind';
import dayjs from './dayjs';
import { downtimeWindows, lifecycleMarkers, splitSegments } from './downtime';

const T0 = Date.UTC(2026, 0, 1, 0, 0, 0);
const MIN = 60_000;

function event(kind: SystemEventKind, at: number): SystemEvent {
  return { id: at, kind, details: {}, created_at: dayjs(at).toISOString() };
}

function point(at: number, value = 1): MempoolMetricPoint {
  return { sampled_at: dayjs(at).toISOString(), value };
}

describe('downtimeWindows', () => {
  it('pairs a stopped/started event into one window', () => {
    const events = [
      event('server_stopped', T0),
      event('server_started', T0 + 5 * MIN),
    ];
    expect(downtimeWindows(events, T0 + 10 * MIN)).toEqual([
      { from: T0, to: T0 + 5 * MIN },
    ]);
  });

  it('closes a still-open window at domainTo -- the server is down right now', () => {
    const events = [event('server_stopped', T0)];
    expect(downtimeWindows(events, T0 + 10 * MIN)).toEqual([
      { from: T0, to: T0 + 10 * MIN },
    ]);
  });

  it('ignores a lone server_started -- first boot / fresh DB, not an outage', () => {
    const events = [event('server_started', T0)];
    expect(downtimeWindows(events, T0 + 10 * MIN)).toEqual([]);
  });

  it('keeps the first of two consecutive server_stopped as the window start', () => {
    const events = [
      event('server_stopped', T0),
      event('server_stopped', T0 + 2 * MIN),
      event('server_started', T0 + 5 * MIN),
    ];
    expect(downtimeWindows(events, T0 + 10 * MIN)).toEqual([
      { from: T0, to: T0 + 5 * MIN },
    ]);
  });

  it('ignores unrelated event kinds', () => {
    const events = [
      event('server_stopped', T0),
      event('node_connected', T0 + 1 * MIN),
      event('bootstrap_started', T0 + 2 * MIN),
      event('server_started', T0 + 5 * MIN),
    ];
    expect(downtimeWindows(events, T0 + 10 * MIN)).toEqual([
      { from: T0, to: T0 + 5 * MIN },
    ]);
  });

  it('sorts unsorted input before pairing', () => {
    const events = [
      event('server_started', T0 + 5 * MIN),
      event('server_stopped', T0),
    ];
    expect(downtimeWindows(events, T0 + 10 * MIN)).toEqual([
      { from: T0, to: T0 + 5 * MIN },
    ]);
  });

  it('does not emit a zero-length window', () => {
    const events = [event('server_stopped', T0), event('server_started', T0)];
    expect(downtimeWindows(events, T0 + 10 * MIN)).toEqual([]);
  });
});

describe('lifecycleMarkers', () => {
  it('keeps only server_started/server_stopped, sorted by created_at', () => {
    const events = [
      event('server_started', T0 + 5 * MIN),
      event('node_connected', T0 + 1 * MIN),
      event('server_stopped', T0),
      event('bootstrap_completed', T0 + 3 * MIN),
    ];
    expect(lifecycleMarkers(events)).toEqual([
      event('server_stopped', T0),
      event('server_started', T0 + 5 * MIN),
    ]);
  });
});

describe('splitSegments', () => {
  const RES = 60; // seconds -- 2x is 120_000 ms, so MIN-sized gaps below are "healthy"

  it('returns [] for empty input', () => {
    expect(splitSegments([], RES, [])).toEqual([]);
  });

  it('keeps a healthy evenly-spaced series as one run', () => {
    const points = [point(T0), point(T0 + MIN), point(T0 + 2 * MIN)];
    expect(splitSegments(points, RES, [])).toEqual([points]);
  });

  it('breaks across a downtime window', () => {
    const points = [point(T0), point(T0 + MIN)];
    const windows = [{ from: T0 + 100, to: T0 + MIN - 100 }];
    expect(splitSegments(points, RES, windows)).toEqual([
      [points[0]],
      [points[1]],
    ]);
  });

  it('ignores a downtime window outside the pair interval', () => {
    const points = [point(T0), point(T0 + MIN)];
    const windows = [{ from: T0 - 10 * MIN, to: T0 - 5 * MIN }];
    expect(splitSegments(points, RES, windows)).toEqual([points]);
  });

  it('breaks on a >2x-resolution hole with no events at all (crash case)', () => {
    const points = [point(T0), point(T0 + 2 * MIN + 1000)];
    expect(splitSegments(points, RES, [])).toEqual([[points[0]], [points[1]]]);
  });

  it('does not break at exactly 2x resolution (boundary)', () => {
    const points = [point(T0), point(T0 + 2 * MIN)];
    expect(splitSegments(points, RES, [])).toEqual([points]);
  });

  it('emits a single stranded point as its own 1-element run between two healthy pairs', () => {
    const p0 = point(T0);
    const p1 = point(T0 + MIN); // healthy with p0
    const p2 = point(T0 + 6 * MIN); // >2x from p1 on both sides -- stranded
    const p3 = point(T0 + 11 * MIN);
    const p4 = point(T0 + 12 * MIN); // healthy with p3
    expect(splitSegments([p0, p1, p2, p3, p4], RES, [])).toEqual([
      [p0, p1],
      [p2],
      [p3, p4],
    ]);
  });

  it('does not mutate the input array', () => {
    const points = [point(T0), point(T0 + MIN)];
    const original = [...points];
    splitSegments(points, RES, []);
    expect(points).toEqual(original);
  });
});
