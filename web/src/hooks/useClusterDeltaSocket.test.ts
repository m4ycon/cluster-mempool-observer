import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ClusterDeltaEvent, ClusterRef } from '../types/events';
import { useClusterDeltaSocket } from './useClusterDeltaSocket';

// The hook only reads `lastJsonMessage`/`readyState`, so a mutable stand-in
// plus a rerender is enough to play messages through it.
const socket = {
  lastJsonMessage: null as ClusterDeltaEvent | null,
  readyState: 1,
};

vi.mock('react-use-websocket', () => ({
  default: () => socket,
}));

function cluster(id: number, overrides: Partial<ClusterRef> = {}): ClusterRef {
  return {
    id,
    txids: [],
    total_vsize: 1000,
    total_fee: 1000,
    ...overrides,
  };
}

function delta(
  upserted: ClusterRef[],
  removed: number[] = [],
): ClusterDeltaEvent {
  return { upserted, removed: removed.map((id) => BigInt(id)) };
}

function setup() {
  const view = renderHook(() => useClusterDeltaSocket());

  const send = (event: ClusterDeltaEvent) => {
    socket.lastJsonMessage = event;
    view.rerender();
  };

  return { ...view, send };
}

beforeEach(() => {
  socket.lastJsonMessage = null;
});

describe('useClusterDeltaSocket', () => {
  it('applies upserts and removals while live', () => {
    const { result, send } = setup();

    send(delta([cluster(1), cluster(2)]));
    expect(result.current.clusters.map((c) => c.id)).toEqual([1, 2]);

    send(delta([cluster(1, { total_vsize: 42 })], [2]));
    expect(result.current.clusters).toEqual([cluster(1, { total_vsize: 42 })]);
  });

  it('starts live', () => {
    const { result } = setup();
    expect(result.current.paused).toBe(false);
  });

  it('freezes the list while paused', () => {
    const { result, send } = setup();

    send(delta([cluster(1)]));
    act(() => result.current.togglePaused());

    send(delta([cluster(2)], [1]));
    expect(result.current.paused).toBe(true);
    expect(result.current.clusters.map((c) => c.id)).toEqual([1]);
  });

  it('keeps the frozen array identity across deltas', () => {
    const { result, send } = setup();

    send(delta([cluster(1)]));
    act(() => result.current.togglePaused());

    const frozen = result.current.clusters;
    send(delta([cluster(2)]));
    send(delta([cluster(3)]));

    expect(result.current.clusters).toBe(frozen);
  });

  it('keeps togglePaused identity across deltas', () => {
    const { result, send } = setup();

    const toggle = result.current.togglePaused;
    send(delta([cluster(1)]));
    send(delta([cluster(2)]));

    expect(result.current.togglePaused).toBe(toggle);
  });

  it('jumps to the present on resume instead of replaying', () => {
    const { result, send } = setup();

    send(delta([cluster(1)]));
    act(() => result.current.togglePaused());

    send(delta([cluster(2), cluster(3)], [1]));
    act(() => result.current.togglePaused());

    expect(result.current.paused).toBe(false);
    expect(result.current.clusters.map((c) => c.id)).toEqual([2, 3]);
  });
});
