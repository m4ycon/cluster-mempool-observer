import { describe, expect, it } from 'vitest';
import type { GaugePoint } from '../types/generated/GaugePoint';
import type { SystemEvent } from '../types/generated/SystemEvent';
import type { SystemEventKind } from '../types/generated/SystemEventKind';
import dayjs from './dayjs';
import { type GaugeLayoutInput, gaugeLayout } from './gaugeChart';

const T0 = Date.UTC(2026, 0, 1, 0, 0, 0);
const MIN = 60_000;
const W = 960;
const H = 320;

function point(at: number, value: number | null = 1): GaugePoint {
  return { sampled_at: dayjs(at).toISOString(), value };
}

function event(kind: SystemEventKind, at: number): SystemEvent {
  return { id: at, kind, details: {}, created_at: dayjs(at).toISOString() };
}

function input({
  points = [],
  ...overrides
}: Partial<GaugeLayoutInput> & {
  /** Shorthand for a single series; `series` wins when both are given. */
  points?: GaugePoint[];
} = {}): GaugeLayoutInput {
  return {
    series: [points],
    resolutionSecs: 60,
    events: [],
    domain: { from: T0, to: T0 + 10 * MIN },
    ...overrides,
  };
}

describe('gaugeLayout', () => {
  it('maps x against the requested domain, not the data extent', () => {
    const domain = { from: T0, to: T0 + 20 * MIN };
    const points = [
      point(T0 + 1 * MIN),
      point(T0 + 4 * MIN),
      point(T0 + 8 * MIN),
    ];
    const layout = gaugeLayout(input({ points, domain }), W, H);

    // All 3 points sit in the first 8/20 of the window -- if the scale had
    // domained on the data extent instead, the last point would land at the
    // plot's right edge rather than comfortably left of its midpoint.
    const midpoint = layout.plot.left + layout.plot.width / 2;
    for (const p of layout.lines[0].points) {
      expect(p.px).toBeLessThan(midpoint);
    }
  });

  it('an ongoing outage (no later server_started) leaves the right side of the plot uncovered', () => {
    const domain = { from: T0, to: T0 + 20 * MIN };
    const points = [point(T0), point(T0 + 1 * MIN), point(T0 + 5 * MIN)];
    const events = [event('server_stopped', T0 + 5 * MIN)];
    const layout = gaugeLayout(
      input({ points, events, domain, resolutionSecs: 300 }),
      W,
      H,
    );

    expect(layout.lines[0].segments).toHaveLength(1);
    const midpoint = layout.plot.left + layout.plot.width / 2;
    for (const p of layout.lines[0].segments[0].points) {
      expect(p.px).toBeLessThan(midpoint);
    }
  });

  it('a clean stopped/started pair yields 2 segments, not 1', () => {
    const domain = { from: T0, to: T0 + 20 * MIN };
    const points = [
      point(T0),
      point(T0 + 1 * MIN),
      point(T0 + 10 * MIN),
      point(T0 + 11 * MIN),
    ];
    const events = [
      event('server_stopped', T0 + 2 * MIN),
      event('server_started', T0 + 9 * MIN),
    ];
    const layout = gaugeLayout(input({ points, events, domain }), W, H);

    expect(layout.lines[0].segments).toHaveLength(2);
    expect(layout.lines[0].segments[0].points).toHaveLength(2);
    expect(layout.lines[0].segments[1].points).toHaveLength(2);
  });

  it('a healthy series with no gaps or events yields exactly 1 segment', () => {
    const domain = { from: T0, to: T0 + 10 * MIN };
    const points = [
      point(T0),
      point(T0 + 1 * MIN),
      point(T0 + 2 * MIN),
      point(T0 + 3 * MIN),
    ];
    const layout = gaugeLayout(input({ points, domain }), W, H);

    expect(layout.lines[0].segments).toHaveLength(1);
    expect(layout.lines[0].segments[0].points).toHaveLength(4);
  });

  it("each segment's areaPath closes on its own endpoints, not spanning the gap", () => {
    const domain = { from: T0, to: T0 + 20 * MIN };
    const points = [
      point(T0),
      point(T0 + 1 * MIN),
      point(T0 + 10 * MIN),
      point(T0 + 11 * MIN),
    ];
    const events = [
      event('server_stopped', T0 + 2 * MIN),
      event('server_started', T0 + 9 * MIN),
    ];
    const layout = gaugeLayout(input({ points, events, domain }), W, H);

    expect(layout.lines[0].segments).toHaveLength(2);
    const [seg0, seg1] = layout.lines[0].segments;
    const seg0First = seg0.points[0];
    const seg0Last = seg0.points[seg0.points.length - 1];
    const seg1First = seg1.points[0];
    const seg1Last = seg1.points[seg1.points.length - 1];

    // Segment 0 closes on its own last point, never on segment 1's (the global last).
    expect(seg0.areaPath).toContain(`L${seg0Last.px.toFixed(1)},`);
    expect(seg0.areaPath).not.toContain(`L${seg1Last.px.toFixed(1)},`);

    // Segment 1 closes on its own first point, never on segment 0's (the global first).
    expect(seg1.areaPath).toContain(`L${seg1First.px.toFixed(1)},`);
    expect(seg1.areaPath).not.toContain(`L${seg0First.px.toFixed(1)},`);
  });

  it('leaves samples without a reading out, so even one splits the line', () => {
    const points = [
      point(T0 + 1 * MIN),
      point(T0 + 2 * MIN),
      point(T0 + 3 * MIN, null),
      point(T0 + 4 * MIN),
    ];
    const layout = gaugeLayout(input({ points }), W, H);

    expect(layout.lines[0].points).toHaveLength(3);
    expect(layout.lines[0].segments.map((s) => s.points.length)).toEqual([
      2, 1,
    ]);
  });

  it('a stranded single point becomes its own 1-point segment', () => {
    const domain = { from: T0, to: T0 + 15 * MIN };
    const p0 = point(T0);
    const p1 = point(T0 + 1 * MIN); // healthy with p0
    const p2 = point(T0 + 6 * MIN); // stranded -- >2x resolution from both neighbours
    const p3 = point(T0 + 11 * MIN);
    const p4 = point(T0 + 12 * MIN); // healthy with p3
    const layout = gaugeLayout(
      input({ points: [p0, p1, p2, p3, p4], domain }),
      W,
      H,
    );

    expect(layout.lines[0].segments).toHaveLength(3);
    expect(layout.lines[0].segments[1].points).toHaveLength(1);
    expect(layout.lines[0].segments[1].points[0].point).toEqual(p2);
  });

  it('maps markers to px inside the plot rect and drops one outside the domain', () => {
    const domain = { from: T0, to: T0 + 10 * MIN };
    const events = [
      event('server_stopped', T0 + 3 * MIN),
      event('server_started', T0 + 20 * MIN), // after domain.to -- dropped
    ];
    const layout = gaugeLayout(
      input({ points: [point(T0), point(T0 + 1 * MIN)], events, domain }),
      W,
      H,
    );

    expect(layout.markers).toHaveLength(1);
    expect(layout.markers[0].event.kind).toBe('server_stopped');
    expect(layout.markers[0].px).toBeGreaterThanOrEqual(layout.plot.left);
    expect(layout.markers[0].px).toBeLessThanOrEqual(
      layout.plot.left + layout.plot.width,
    );
  });

  it('stays total for empty points -- valid plot/ticks/markers, no crash', () => {
    const domain = { from: T0, to: T0 + 10 * MIN };
    const events = [event('server_stopped', T0 + 3 * MIN)];
    const layout = gaugeLayout(input({ points: [], events, domain }), W, H);

    expect(layout.lines[0].points).toEqual([]);
    expect(layout.lines[0].segments).toEqual([]);
    expect(layout.gaps).toHaveLength(1);
    expect(layout.plot.width).toBeGreaterThan(0);
    expect(layout.plot.height).toBeGreaterThan(0);
    expect(layout.xTicks.length).toBeGreaterThan(0);
    expect(layout.yTicks.length).toBeGreaterThan(0);
    expect(layout.markers).toHaveLength(1);
  });
});

describe('gaugeLayout gaps', () => {
  const domain = { from: T0, to: T0 + 10 * MIN };
  const xOf = (layout: ReturnType<typeof gaugeLayout>, at: number) =>
    layout.plot.left +
    ((at - domain.from) / (domain.to - domain.from)) * layout.plot.width;

  it('has none for a series that covers its whole range', () => {
    const points = Array.from({ length: 11 }, (_, i) => point(T0 + i * MIN));
    const layout = gaugeLayout(input({ points, domain }), W, H);

    expect(layout.gaps).toEqual([]);
  });

  it('spans a run without a reading from the last reading to the next one', () => {
    const points = Array.from({ length: 11 }, (_, i) =>
      point(T0 + i * MIN, i === 4 || i === 5 ? null : 1),
    );
    const layout = gaugeLayout(input({ points, domain }), W, H);

    expect(layout.gaps).toHaveLength(1);
    expect(layout.gaps[0].x0).toBeCloseTo(xOf(layout, T0 + 3 * MIN));
    expect(layout.gaps[0].x1).toBeCloseTo(xOf(layout, T0 + 6 * MIN));
  });

  it('spans a downtime window', () => {
    const points = [
      ...Array.from({ length: 3 }, (_, i) => point(T0 + i * MIN)),
      ...Array.from({ length: 3 }, (_, i) => point(T0 + (8 + i) * MIN)),
    ];
    const events = [
      event('server_stopped', T0 + 2.5 * MIN),
      event('server_started', T0 + 7.5 * MIN),
    ];
    const layout = gaugeLayout(input({ points, events, domain }), W, H);

    expect(layout.gaps).toHaveLength(1);
    expect(layout.gaps[0].x0).toBeCloseTo(xOf(layout, T0 + 2 * MIN));
    expect(layout.gaps[0].x1).toBeCloseTo(xOf(layout, T0 + 8 * MIN));
  });

  it('covers the start and the end of the range when no reading reaches them', () => {
    const points = [
      point(T0 + 4 * MIN, null),
      point(T0 + 5 * MIN),
      point(T0 + 6 * MIN),
    ];
    const layout = gaugeLayout(input({ points, domain }), W, H);

    expect(layout.gaps).toHaveLength(2);
    expect(layout.gaps[0].x0).toBeCloseTo(xOf(layout, domain.from));
    expect(layout.gaps[0].x1).toBeCloseTo(xOf(layout, T0 + 5 * MIN));
    expect(layout.gaps[1].x0).toBeCloseTo(xOf(layout, T0 + 6 * MIN));
    expect(layout.gaps[1].x1).toBeCloseTo(xOf(layout, domain.to));
  });

  it('leaves the newest sample a resolution of slack before the end of the range', () => {
    const points = Array.from({ length: 10 }, (_, i) => point(T0 + i * MIN));
    const layout = gaugeLayout(input({ points, domain }), W, H);

    expect(layout.gaps).toEqual([]);
  });

  it('is a gap only where no series holds a reading', () => {
    const times = Array.from({ length: 11 }, (_, i) => T0 + i * MIN);
    const a = times.map((t, i) => point(t, i === 4 || i === 5 ? null : 1));
    const b = times.map((t, i) => point(t, i === 5 ? null : 2));
    const layout = gaugeLayout(input({ domain, series: [a, b] }), W, H);

    expect(layout.gaps).toHaveLength(1);
    expect(layout.gaps[0].x0).toBeCloseTo(xOf(layout, T0 + 4 * MIN));
    expect(layout.gaps[0].x1).toBeCloseTo(xOf(layout, T0 + 6 * MIN));
  });
});

describe('gaugeLayout with several series', () => {
  it('lays each series out on one shared y axis', () => {
    const domain = { from: T0, to: T0 + 2 * MIN };
    const low = [point(T0, 1), point(T0 + MIN, 1), point(T0 + 2 * MIN, 1)];
    const high = [point(T0, 4), point(T0 + MIN, 4), point(T0 + 2 * MIN, 4)];
    const layout = gaugeLayout(input({ domain, series: [low, high] }), W, H);

    expect(layout.lines).toHaveLength(2);
    expect(layout.lines[1].points[0].py).toBeCloseTo(layout.plot.top);
    expect(layout.lines[0].points[0].py).toBeGreaterThan(
      layout.lines[1].points[0].py,
    );
  });
});
