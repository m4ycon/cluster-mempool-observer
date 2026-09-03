import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
      linked={true}
      onLinkedChange={vi.fn()}
      includeSingletons={true}
      onIncludeSingletonsChange={onIncludeSingletonsChange}
      {...overrides}
    />,
  );
  return { ...utils, onIncludeSingletonsChange };
}

describe('ClusterControls', () => {
  it('renders the BINS slider and no COLOR BY select for the histogram viz', () => {
    renderControls('histogram');

    expect(screen.getByText('BINS')).toBeInTheDocument();
    expect(screen.getByText('BIN BY')).toBeInTheDocument();
    expect(screen.queryByText('COLOR BY')).not.toBeInTheDocument();
    expect(screen.queryByText('SIZE/COLOR BY')).not.toBeInTheDocument();
    expect(screen.queryByText('SHOW')).not.toBeInTheDocument();
  });

  it('renders the SHOW slider (and no BINS slider) for circles', () => {
    renderControls('circles');

    expect(screen.getByText('SHOW')).toBeInTheDocument();
    expect(screen.queryByText('BINS')).not.toBeInTheDocument();
    expect(screen.queryByText('BIN BY')).not.toBeInTheDocument();
  });

  it('renders the SHOW slider (and no BINS slider) for treemap', () => {
    renderControls('treemap');

    expect(screen.getByText('SHOW')).toBeInTheDocument();
    expect(screen.queryByText('BINS')).not.toBeInTheDocument();
    expect(screen.queryByText('BIN BY')).not.toBeInTheDocument();
  });

  it('shows the SINGLETONS toggle on every viz, table included', () => {
    for (const viz of ['circles', 'treemap', 'histogram', 'table'] as const) {
      const { unmount } = renderControls(viz);
      expect(
        screen.getByRole('button', { name: 'Exclude singleton clusters' }),
      ).toHaveAttribute('aria-pressed', 'true');
      unmount();
    }
  });

  it('flips the singleton flag on click', async () => {
    const user = userEvent.setup();
    const { onIncludeSingletonsChange } = renderControls('circles', {
      includeSingletons: false,
    });

    await user.click(
      screen.getByRole('button', { name: 'Include singleton clusters' }),
    );
    expect(onIncludeSingletonsChange).toHaveBeenCalledWith(true);
  });
});
