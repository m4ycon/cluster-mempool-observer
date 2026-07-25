import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ClusterControls } from './ClusterControls';

function renderControls(vizType: 'circles' | 'treemap' | 'histogram') {
  return render(
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
    />,
  );
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
});
