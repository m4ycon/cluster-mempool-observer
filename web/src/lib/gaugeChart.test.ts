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

function point(at: number, value = 1): GaugePoint {
  return { sampled_at: dayjs(at).toISOString(), value };
}

function event(kind: SystemEventKind, at: number): SystemEvent {
  return { id: at, kind, details: {}, created_at: dayjs(at).toISOString() };
}

function input(overrides: Partial<GaugeLayoutInput> = {}): GaugeLayoutInput {
  return {
    points: [],
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
    for (const p of layout.points) {
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

    expect(layout.segments).toHaveLength(1);
    const midpoint = layout.plot.left + layout.plot.width / 2;
    for (const p of layout.segments[0].points) {
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

    expect(layout.segments).toHaveLength(2);
    expect(layout.segments[0].points).toHaveLength(2);
    expect(layout.segments[1].points).toHaveLength(2);
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

    expect(layout.segments).toHaveLength(1);
    expect(layout.segments[0].points).toHaveLength(4);
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

    expect(layout.segments).toHaveLength(2);
    const [seg0, seg1] = layout.segments;
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

    expect(layout.segments).toHaveLength(3);
    expect(layout.segments[1].points).toHaveLength(1);
    expect(layout.segments[1].points[0].point).toEqual(p2);
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

    expect(layout.points).toEqual([]);
    expect(layout.segments).toEqual([]);
    expect(layout.plot.width).toBeGreaterThan(0);
    expect(layout.plot.height).toBeGreaterThan(0);
    expect(layout.xTicks.length).toBeGreaterThan(0);
    expect(layout.yTicks.length).toBeGreaterThan(0);
    expect(layout.markers).toHaveLength(1);
  });
});
