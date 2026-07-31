import { render, screen } from '@testing-library/react';
import { ReadyState } from 'react-use-websocket';
import { describe, expect, it } from 'vitest';
import { ConnectionDot } from './ConnectionDot';

function renderDot(readyState: ReadyState) {
  const { unmount } = render(
    <ConnectionDot readyState={readyState} label="STATS FEED" />,
  );
  return {
    core: screen.getByRole('status'),
    halo: screen.queryByTestId('connection-halo'),
    unmount,
  };
}

describe('ConnectionDot', () => {
  it('holds a steady live-green core behind a calm halo while the socket is open', () => {
    const { core, halo } = renderDot(ReadyState.OPEN);

    expect(core).toHaveAttribute('aria-label', 'STATS FEED · LIVE');
    expect(core).toHaveClass('bg-live');
    expect(core.className).not.toMatch(/animate-/);
    expect(halo).toHaveClass('bg-live', 'animate-ping-live');
    expect(screen.getByRole('tooltip')).toHaveTextContent('STATS FEED · LIVE');
  });

  it.each([
    ['connecting', ReadyState.CONNECTING],
    // Both sockets reconnect for good, so a closing one is not offline yet.
    ['closing', ReadyState.CLOSING],
  ])('blinks the core over a fast halo while %s', (_name, readyState) => {
    const { core, halo } = renderDot(readyState);

    expect(core).toHaveAttribute('aria-label', 'STATS FEED · RECONNECTING');
    expect(core).toHaveClass('bg-slate', 'animate-blink-fast');
    expect(halo).toHaveClass('bg-slate', 'animate-ping-busy');
  });

  it.each([
    ['closed', ReadyState.CLOSED],
    ['uninstantiated', ReadyState.UNINSTANTIATED],
  ])('drops the halo and sits still while %s', (_name, readyState) => {
    const { core, halo } = renderDot(readyState);

    expect(core).toHaveAttribute('aria-label', 'STATS FEED · OFFLINE');
    expect(core).toHaveClass('bg-alert');
    expect(core.className).not.toMatch(/animate-/);
    expect(halo).not.toBeInTheDocument();
  });

  it('never puts two animations on one element', () => {
    for (const readyState of [
      ReadyState.OPEN,
      ReadyState.CONNECTING,
      ReadyState.CLOSING,
      ReadyState.CLOSED,
    ]) {
      const { core, halo, unmount } = renderDot(readyState);

      for (const el of [core, halo]) {
        const animations = el?.className.match(/animate-[\w-]+/g) ?? [];
        expect(animations.length).toBeLessThanOrEqual(1);
      }

      unmount();
    }
  });

  it('passes the tooltip alignment through', () => {
    render(
      <ConnectionDot
        readyState={ReadyState.OPEN}
        label="CLUSTER FEED"
        align="right"
      />,
    );

    expect(screen.getByRole('tooltip')).toHaveClass('right-0');
  });
});
