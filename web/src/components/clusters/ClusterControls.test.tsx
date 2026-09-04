import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ReadyState } from 'react-use-websocket';
import { describe, expect, it, vi } from 'vitest';
import type { VizType } from '../../lib/clustersSearch';
import { ClusterControls } from './ClusterControls';

function renderControls(
  vizType: VizType,
  overrides: Partial<Parameters<typeof ClusterControls>[0]> = {},
) {
  const onIncludeSingletonsChange = vi.fn();
  const utils = render(
    <ClusterControls
      vizType={vizType}
      onVizTypeChange={vi.fn()}
      sizeMetric="feerate"
      onSizeMetricChange={vi.fn()}
      colorMetric="feerate"
      onColorMetricChange={vi.fn()}
      showCount={40}
      onShowCountChange={vi.fn()}
      bins={20}
      onBinsChange={vi.fn()}
      paused={false}
      onTogglePause={vi.fn()}
      readyState={ReadyState.OPEN}
      linked={true}
      onLinkedChange={vi.fn()}
      includeSingletons={true}
      onIncludeSingletonsChange={onIncludeSingletonsChange}
      {...overrides}
    />,
  );
  return { ...utils, onIncludeSingletonsChange };
}

/** The filter controls live behind the popover trigger; open it first. */
async function openFilters() {
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: 'Toggle filters' }));
  return user;
}

describe('ClusterControls', () => {
  it('renders the BINS slider and no COLOR BY select for the histogram viz', async () => {
    renderControls('histogram');
    await openFilters();

    expect(screen.getByText('BINS')).toBeInTheDocument();
    expect(screen.getByText('BIN BY')).toBeInTheDocument();
    expect(screen.queryByText('COLOR BY')).not.toBeInTheDocument();
    expect(screen.queryByText('SIZE/COLOR BY')).not.toBeInTheDocument();
    expect(screen.queryByText('SHOW')).not.toBeInTheDocument();
  });

  it('renders the SHOW slider (and no BINS slider) for circles', async () => {
    renderControls('circles');
    await openFilters();

    expect(screen.getByText('SHOW')).toBeInTheDocument();
    expect(screen.queryByText('BINS')).not.toBeInTheDocument();
    expect(screen.queryByText('BIN BY')).not.toBeInTheDocument();
  });

  it('renders the SHOW slider (and no BINS slider) for treemap', async () => {
    renderControls('treemap');
    await openFilters();

    expect(screen.getByText('SHOW')).toBeInTheDocument();
    expect(screen.queryByText('BINS')).not.toBeInTheDocument();
    expect(screen.queryByText('BIN BY')).not.toBeInTheDocument();
  });

  it('shows the SINGLETONS toggle on every viz, table included', async () => {
    for (const viz of ['circles', 'treemap', 'histogram', 'table'] as const) {
      const { unmount } = renderControls(viz);
      await openFilters();
      expect(
        screen.getByRole('button', { name: 'Exclude singleton clusters' }),
      ).toHaveAttribute('aria-pressed', 'true');
      unmount();
    }
  });

  it('flips the singleton flag on click', async () => {
    const { onIncludeSingletonsChange } = renderControls('circles', {
      includeSingletons: false,
    });
    const user = await openFilters();

    await user.click(
      screen.getByRole('button', { name: 'Include singleton clusters' }),
    );
    expect(onIncludeSingletonsChange).toHaveBeenCalledWith(true);
  });

  it('opens the filter popover on trigger click and closes it on Escape', async () => {
    renderControls('circles');
    const user = userEvent.setup();

    expect(screen.queryByText('SHOW')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Toggle filters' }));
    expect(screen.getByText('SHOW')).toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(screen.queryByText('SHOW')).not.toBeInTheDocument();
  });

  it('closes the filter popover on an outside click', async () => {
    renderControls('circles');
    const user = await openFilters();

    expect(screen.getByText('SHOW')).toBeInTheDocument();

    await user.click(document.body);
    expect(screen.queryByText('SHOW')).not.toBeInTheDocument();
  });
});
