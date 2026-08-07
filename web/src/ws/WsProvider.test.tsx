import { act, render, renderHook } from '@testing-library/react';
import { ReadyState } from 'react-use-websocket';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useClusterDeltaSocket } from '../hooks/useClusterDeltaSocket';
import { silenceConsoleError } from '../test/console';
import type {
  ClusterDeltaEvent,
  ClusterRef,
  MempoolStatsEvent,
  ServerEvent,
  WsSubject,
} from '../types/events';
import type { PayloadFor } from './payload';
import { useSubscription } from './useSubscription';
import { WsProvider } from './WsProvider';

const wsMock = {
  sendJsonMessage: vi.fn(),
  readyState: ReadyState.CONNECTING as ReadyState,
  onMessage: undefined as ((event: MessageEvent) => void) | undefined,
};

vi.mock('react-use-websocket', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-use-websocket')>();
  return {
    ...actual,
    default: (
      _url: string,
      options: { onMessage?: (event: MessageEvent) => void },
    ) => {
      wsMock.onMessage = options.onMessage;
      return {
        sendJsonMessage: wsMock.sendJsonMessage,
        readyState: wsMock.readyState,
      };
    },
  };
});

beforeEach(() => {
  wsMock.sendJsonMessage.mockClear();
  wsMock.readyState = ReadyState.OPEN;
  wsMock.onMessage = undefined;
});

/** Subscribes to `subject` for as long as it is mounted; renders nothing. */
function Subscriber<S extends WsSubject>({
  subject,
  onEvent = () => {},
}: {
  subject: S;
  onEvent?: (payload: PayloadFor<S>) => void;
}) {
  useSubscription(subject, onEvent);
  return null;
}

function frame(event: ServerEvent): MessageEvent {
  return new MessageEvent('message', { data: JSON.stringify(event) });
}

let inlineHandlerCalls: number[] = [];

function InlineHandlerSubscriber({ tag }: { tag: number }) {
  // A fresh closure every render -- the point of this component is that its
  // identity changes on every render, and useSubscription must still call
  // the latest one without resubscribing.
  useSubscription('cluster.delta', () => {
    inlineHandlerCalls.push(tag);
  });
  return null;
}

describe('WsProvider', () => {
  describe('refcounting', () => {
    it('sends exactly one subscribe frame for the first subscriber to a subject', () => {
      render(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );

      expect(wsMock.sendJsonMessage).toHaveBeenCalledTimes(1);
      expect(wsMock.sendJsonMessage).toHaveBeenCalledWith({
        action: 'subscribe',
        subject: 'cluster.delta',
      });
    });

    it('sends no additional frame for a second subscriber to the same subject', () => {
      const { rerender } = render(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );
      wsMock.sendJsonMessage.mockClear();

      rerender(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );

      expect(wsMock.sendJsonMessage).not.toHaveBeenCalled();
    });

    it('sends no unsubscribe when one of two subscribers to a subject unmounts, since the other is still listening', () => {
      const { rerender } = render(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );
      wsMock.sendJsonMessage.mockClear();

      rerender(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );

      expect(wsMock.sendJsonMessage).not.toHaveBeenCalled();
    });

    it('sends an unsubscribe frame when the last subscriber to a subject unmounts', () => {
      const { rerender } = render(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );
      wsMock.sendJsonMessage.mockClear();

      rerender(<WsProvider>{null}</WsProvider>);

      expect(wsMock.sendJsonMessage).toHaveBeenCalledTimes(1);
      expect(wsMock.sendJsonMessage).toHaveBeenCalledWith({
        action: 'unsubscribe',
        subject: 'cluster.delta',
      });
    });

    it('sends a separate subscribe frame for each distinct subject', () => {
      render(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
          <Subscriber subject="mempool.stats" />
        </WsProvider>,
      );

      expect(wsMock.sendJsonMessage).toHaveBeenCalledTimes(2);
      expect(wsMock.sendJsonMessage).toHaveBeenCalledWith({
        action: 'subscribe',
        subject: 'cluster.delta',
      });
      expect(wsMock.sendJsonMessage).toHaveBeenCalledWith({
        action: 'subscribe',
        subject: 'mempool.stats',
      });
    });
  });

  describe('connection lifecycle', () => {
    it('sends no frame for a subscribe while CONNECTING, then sends it once the socket opens', () => {
      wsMock.readyState = ReadyState.CONNECTING;

      const { rerender } = render(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );
      expect(wsMock.sendJsonMessage).not.toHaveBeenCalled();

      wsMock.readyState = ReadyState.OPEN;
      rerender(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
        </WsProvider>,
      );

      expect(wsMock.sendJsonMessage).toHaveBeenCalledTimes(1);
      expect(wsMock.sendJsonMessage).toHaveBeenCalledWith({
        action: 'subscribe',
        subject: 'cluster.delta',
      });
    });

    it('re-subscribes every subject still in the registry after the socket cycles OPEN -> CLOSED -> OPEN', () => {
      const { rerender } = render(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
          <Subscriber subject="mempool.stats" />
        </WsProvider>,
      );
      wsMock.sendJsonMessage.mockClear();

      wsMock.readyState = ReadyState.CLOSED;
      rerender(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
          <Subscriber subject="mempool.stats" />
        </WsProvider>,
      );
      expect(wsMock.sendJsonMessage).not.toHaveBeenCalled();

      wsMock.readyState = ReadyState.OPEN;
      rerender(
        <WsProvider>
          <Subscriber subject="cluster.delta" />
          <Subscriber subject="mempool.stats" />
        </WsProvider>,
      );

      expect(wsMock.sendJsonMessage).toHaveBeenCalledTimes(2);
      expect(wsMock.sendJsonMessage).toHaveBeenCalledWith({
        action: 'subscribe',
        subject: 'cluster.delta',
      });
      expect(wsMock.sendJsonMessage).toHaveBeenCalledWith({
        action: 'subscribe',
        subject: 'mempool.stats',
      });
    });
  });

  describe('dispatch', () => {
    it('delivers an incoming frame only to the handlers registered for its subject, with the payload unwrapped from the envelope', () => {
      const clusterEvents: ClusterDeltaEvent[] = [];
      const statsEvents: MempoolStatsEvent[] = [];

      render(
        <WsProvider>
          <Subscriber
            subject="cluster.delta"
            onEvent={(p) => clusterEvents.push(p)}
          />
          <Subscriber
            subject="mempool.stats"
            onEvent={(p) => statsEvents.push(p)}
          />
        </WsProvider>,
      );

      const payload: ClusterDeltaEvent = { upserted: [], removed: [] };
      act(() => {
        wsMock.onMessage?.(frame({ subject: 'cluster.delta', payload }));
      });

      expect(clusterEvents).toEqual([payload]);
      expect(statsEvents).toEqual([]);
    });

    it('fires every handler registered for the same subject', () => {
      const first: ClusterDeltaEvent[] = [];
      const second: ClusterDeltaEvent[] = [];

      render(
        <WsProvider>
          <Subscriber subject="cluster.delta" onEvent={(p) => first.push(p)} />
          <Subscriber subject="cluster.delta" onEvent={(p) => second.push(p)} />
        </WsProvider>,
      );

      const payload: ClusterDeltaEvent = { upserted: [], removed: [] };
      act(() => {
        wsMock.onMessage?.(frame({ subject: 'cluster.delta', payload }));
      });

      expect(first).toEqual([payload]);
      expect(second).toEqual([payload]);
    });

    it('invokes no handler for an error frame and does not throw', () => {
      silenceConsoleError();
      const clusterEvents: ClusterDeltaEvent[] = [];

      render(
        <WsProvider>
          <Subscriber
            subject="cluster.delta"
            onEvent={(p) => clusterEvents.push(p)}
          />
        </WsProvider>,
      );

      const errorEvent: ServerEvent = {
        subject: 'error',
        payload: { message: 'boom' },
      };
      expect(() => {
        act(() => {
          wsMock.onMessage?.(frame(errorEvent));
        });
      }).not.toThrow();

      expect(clusterEvents).toEqual([]);
    });

    /** A throw here would take out the socket's message callback for good. */
    it('does not throw on unparseable data', () => {
      silenceConsoleError();

      render(<WsProvider>{null}</WsProvider>);

      expect(() => {
        act(() => {
          wsMock.onMessage?.(
            new MessageEvent('message', { data: '{not json' }),
          );
        });
      }).not.toThrow();
    });
  });

  describe('with a real consumer', () => {
    it("reflects a cluster.delta frame in useClusterDeltaSocket's clusters", () => {
      const { result } = renderHook(() => useClusterDeltaSocket(), {
        wrapper: WsProvider,
      });

      const cluster: ClusterRef = {
        id: 1,
        txids: [],
        total_vsize: 1000,
        total_fee: 1000,
      };
      act(() => {
        wsMock.onMessage?.(
          frame({
            subject: 'cluster.delta',
            payload: { upserted: [cluster], removed: [] },
          }),
        );
      });

      expect(result.current.clusters).toEqual([cluster]);
    });
  });
});

describe('useSubscription ergonomics', () => {
  beforeEach(() => {
    inlineHandlerCalls = [];
  });

  it('keeps a single subscription and invokes the latest handler even though an inline handler changes identity every render', () => {
    const { rerender } = render(
      <WsProvider>
        <InlineHandlerSubscriber tag={1} />
      </WsProvider>,
    );
    rerender(
      <WsProvider>
        <InlineHandlerSubscriber tag={2} />
      </WsProvider>,
    );

    // Two renders, but the effect that sends `subscribe` depends on [subject,
    // subscribe] only -- the handler's changing identity must not retrigger it.
    expect(wsMock.sendJsonMessage).toHaveBeenCalledTimes(1);

    act(() => {
      wsMock.onMessage?.(
        frame({
          subject: 'cluster.delta',
          payload: { upserted: [], removed: [] },
        }),
      );
    });

    expect(inlineHandlerCalls).toEqual([2]);
  });
});
