import { describe, expect, it } from 'vitest';
import type { CounterPoint } from '../types/generated/CounterPoint';
import type { SystemEvent } from '../types/generated/SystemEvent';
import type { SystemEventKind } from '../types/generated/SystemEventKind';
import { type CounterLayoutInput, counterLayout } from './counterChart';
import dayjs from './dayjs';

const T0 = Date.UTC(2026, 0, 1, 0, 0, 0);
const MIN = 60_000;
const W = 960;
const H = 320;

const ALL_VISIBLE = { added_txs: true, confirmed_txs: true, evicted_txs: true };

function point(
  at: number,
  added_txs: number | null = 1,
  confirmed_txs: number | null = 1,
  evicted_txs: number | null = 1,
): CounterPoint {
  return {
    sampled_at: dayjs(at).toISOString(),
    added_txs,
    confirmed_txs,
    evicted_txs,
  };
}

function event(kind: SystemEventKind, at: number): SystemEvent {
  return { id: at, kind, details: {}, created_at: dayjs(at).toISOString() };
}

function input(
  overrides: Partial<CounterLayoutInput> = {},
): CounterLayoutInput {
  return {
    points: [],
    resolutionSecs: 60,
    events: [],
    domain: { from: T0, to: T0 + 10 * MIN },
    visible: ALL_VISIBLE,
    ...overrides,
  };
}

describe('counterLayout', () => {
  it('a null in one series breaks that series line but leaves the others intact', () => {
    const points = [
      point(T0, 1, 1, 1),
      point(T0 + MIN, null, 2, 2),
      point(T0 + 2 * MIN, 3, 3, 3),
    ];
    const layout = counterLayout(input({ points }), W, H);

    expect(layout.series.added_txs.segments).toHaveLength(2);
    expect(layout.series.added_txs.segments[0].points).toHaveLength(1);
    expect(layout.series.added_txs.segments[1].points).toHaveLength(1);

    expect(layout.series.confirmed_txs.segments).toHaveLength(1);
    expect(layout.series.confirmed_txs.segments[0].points).toHaveLength(3);
    expect(layout.series.evicted_txs.segments).toHaveLength(1);
    expect(layout.series.evicted_txs.segments[0].points).toHaveLength(3);
  });

  it('a null bucket breaks the line even when its neighbours sit exactly at the healthy-gap boundary', () => {
    // 1x resolution apart on each side of the null -- 2x total, which
    // splitSegments' own time-gap check treats as still-connected. The null
    // itself must still force a break.
    const points = [
      point(T0, 1, 1, 1),
      point(T0 + MIN, null, 1, 1),
      point(T0 + 2 * MIN, 1, 1, 1),
    ];
    const layout = counterLayout(input({ points }), W, H);
    expect(layout.series.added_txs.segments).toHaveLength(2);
  });

  it('excludes a null from the y-domain -- a visible series with no non-null values contributes nothing', () => {
    const points = [
      point(T0, null, 1, 1),
      point(T0 + MIN, null, 1, 1),
      point(T0 + 2 * MIN, null, 2, 1),
    ];
    // 'added_txs' is visible but null at every point -- if a null leaked into the
    // domain as something other than "absent", the max would not simply track
    // 'confirmed_txs' real values.
    const layout = counterLayout(
      input({
        points,
        visible: { added_txs: true, confirmed_txs: true, evicted_txs: false },
      }),
      W,
      H,
    );
    const maxTick = Math.max(...layout.yTicks.map((t) => t.value));
    expect(maxTick).toBe(2);
    expect(Number.isFinite(maxTick)).toBe(true);
  });

  it('hiding a series widens the y-domain for the rest, and showing it back restores it', () => {
    const points = [point(T0, 1, 1, 1), point(T0 + MIN, 1, 1, 1000)];
    const withEvicted = counterLayout(input({ points }), W, H);
    const withoutEvicted = counterLayout(
      input({
        points,
        visible: { added_txs: true, confirmed_txs: true, evicted_txs: false },
      }),
      W,
      H,
    );

    const maxTick = (l: ReturnType<typeof counterLayout>) =>
      Math.max(...l.yTicks.map((t) => t.value));

    expect(maxTick(withEvicted)).toBeGreaterThan(maxTick(withoutEvicted));

    const restored = counterLayout(input({ points }), W, H);
    expect(maxTick(restored)).toBe(maxTick(withEvicted));
  });

  it('still applies the downtime-window split per series', () => {
    const points = [point(T0), point(T0 + MIN)];
    const events = [
      event('server_stopped', T0 + 100),
      event('server_started', T0 + MIN - 100),
    ];
    const layout = counterLayout(input({ points, events }), W, H);
    expect(layout.series.added_txs.segments).toHaveLength(2);
    expect(layout.series.confirmed_txs.segments).toHaveLength(2);
    expect(layout.series.evicted_txs.segments).toHaveLength(2);
  });

  it('still applies the missing-bucket time-gap split per series', () => {
    const points = [point(T0), point(T0 + 2 * MIN + 1000)];
    const layout = counterLayout(input({ points }), W, H);
    expect(layout.series.added_txs.segments).toHaveLength(2);
  });

  it('an all-null series still produces usable ticks and plot, no NaN paths', () => {
    const points = [
      point(T0, null, null, null),
      point(T0 + MIN, null, null, null),
    ];
    const layout = counterLayout(input({ points }), W, H);

    expect(layout.series.added_txs.segments).toEqual([]);
    expect(layout.series.confirmed_txs.segments).toEqual([]);
    expect(layout.series.evicted_txs.segments).toEqual([]);
    expect(layout.plot.width).toBeGreaterThan(0);
    expect(layout.plot.height).toBeGreaterThan(0);
    expect(layout.xTicks.length).toBeGreaterThan(0);
    expect(layout.yTicks.length).toBeGreaterThan(0);
    expect(layout.xTicks.every((t) => Number.isFinite(t.px))).toBe(true);
    expect(layout.yTicks.every((t) => Number.isFinite(t.px))).toBe(true);
  });

  it('an empty points array still produces usable ticks and plot', () => {
    const layout = counterLayout(input({ points: [] }), W, H);
    expect(layout.hoverPoints).toEqual([]);
    expect(layout.plot.width).toBeGreaterThan(0);
    expect(layout.plot.height).toBeGreaterThan(0);
    expect(layout.xTicks.length).toBeGreaterThan(0);
    expect(layout.yTicks.length).toBeGreaterThan(0);
  });

  it('shares one x-index across all three series for hover hit-testing', () => {
    const points = [
      point(T0, 1, null, 3),
      point(T0 + MIN, 2, 2, null),
      point(T0 + 2 * MIN, null, 4, 4),
    ];
    const layout = counterLayout(input({ points }), W, H);

    expect(layout.hoverPoints).toHaveLength(3);
    layout.hoverPoints.forEach((hp, i) => {
      expect(hp.point).toEqual(points[i]);
    });
    // Ascending in time -> ascending in px on a plain (non-reversed) time scale.
    expect(layout.hoverPoints[0].px).toBeLessThan(layout.hoverPoints[1].px);
    expect(layout.hoverPoints[1].px).toBeLessThan(layout.hoverPoints[2].px);
  });

  it('a lone point isolated by nulls on both sides is still surfaced as its own 1-point segment', () => {
    const points = [
      point(T0, null, 1, 1),
      point(T0 + MIN, 5, 1, 1),
      point(T0 + 2 * MIN, null, 1, 1),
    ];
    const layout = counterLayout(input({ points }), W, H);
    expect(layout.series.added_txs.segments).toHaveLength(1);
    expect(layout.series.added_txs.segments[0].points).toHaveLength(1);
    expect(layout.series.added_txs.segments[0].points[0].point).toEqual(
      points[1],
    );
  });
});
