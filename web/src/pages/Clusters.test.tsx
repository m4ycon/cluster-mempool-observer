import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { ClusterRef } from '../types/events';
import { Clusters } from './Clusters';

// BackLink renders a @tanstack/react-router <Link>, which needs a router
// context this test has no reason to set up -- only its presence matters
// here, not navigation.
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children }: { children?: ReactNode }) => <span>{children}</span>,
}));

// Five distinct total_vsize/total_fee combinations, so sizeMetric and
// colorMetric genuinely produce different values and the colour scale has
// something to tier.
const clusters: ClusterRef[] = [
  { id: 1, txids: ['a1'], total_vsize: 100, total_fee: 500 },
  { id: 2, txids: ['a2'], total_vsize: 300, total_fee: 3000 },
  { id: 3, txids: ['a3'], total_vsize: 600, total_fee: 12000 },
  { id: 4, txids: ['a4'], total_vsize: 900, total_fee: 45000 },
  { id: 5, txids: ['a5'], total_vsize: 1200, total_fee: 96000 },
];

vi.mock('../hooks/useClusterDeltaSocket', () => ({
  useClusterDeltaSocket: () => ({
    clusters,
    readyState: 1, // ReadyState.OPEN -- avoids importing the mocked-around enum
    paused: false,
    togglePaused: vi.fn(),
  }),
}));

describe('Clusters page regression: the linked toggle must survive a viz switch', () => {
  it('keeps the legend tiering by the newly linked metric after treemap -> histogram -> treemap', async () => {
    const user = userEvent.setup();
    render(<Clusters />);

    // Start on treemap: linked defaults to true, a single merged
    // SIZE/COLOR BY select, both metrics default to 'feerate' (unit s/vB).
    await user.click(screen.getByText('TREEMAP'));
    expect(screen.getByLabelText('SIZE/COLOR BY')).toBeInTheDocument();
    expect(screen.getByTestId('cluster-legend').textContent).toContain('s/vB');

    // Switch to histogram -- this unmounts MetricSelects -- and rebind the
    // only metric it exposes (BIN BY, which drives sizeMetric) to TOTAL FEE.
    await user.click(screen.getByText('HISTOGRAM'));
    await user.selectOptions(screen.getByLabelText('BIN BY'), 'fee');

    // Back to treemap: MetricSelects remounts. It shows the merged control,
    // meaning linked is (still) true, so the invariant it advertises --
    // sizeMetric === colorMetric -- must hold: the legend must now tier by
    // 'fee' (sats), not the stale 'feerate' (s/vB) from before the round
    // trip. The merged select's own value can't tell them apart -- it's
    // bound to sizeMetric either way -- so the legend, which is driven by
    // colorMetric alone, is what's actually sensitive to this bug.
    await user.click(screen.getByText('TREEMAP'));

    expect(screen.getByLabelText('SIZE/COLOR BY')).toBeInTheDocument();
    const legend = screen.getByTestId('cluster-legend').textContent ?? '';
    expect(legend).toContain('sats');
    expect(legend).not.toContain('s/vB');
  });
});

describe('Clusters page: histogram swaps the right-hand panel for a distribution summary', () => {
  it('shows the stats panel (not the selected-cluster panel) on histogram, and restores it after switching back', async () => {
    const user = userEvent.setup();
    const { container } = render(<Clusters />);

    const panelLabel = () =>
      container
        .querySelector('[data-screen-label$="panel"]')
        ?.getAttribute('data-screen-label');

    // Circles: the per-cluster panel is showing (TXIDS list only exists there).
    expect(panelLabel()).toBe('Selected cluster panel');
    expect(screen.getByText('TXIDS')).toBeInTheDocument();

    await user.click(screen.getByText('HISTOGRAM'));

    // Histogram: no clicked cluster to describe, so the distribution summary
    // takes over instead -- this is the whole point of this panel swap.
    expect(panelLabel()).toBe('Cluster distribution panel');
    expect(screen.queryByText('TXIDS')).not.toBeInTheDocument();
    expect(screen.getByText('5 CLUSTERS')).toBeInTheDocument();
    // A stats-only label: "TOTAL VSIZE" would also match the BIN BY option.
    expect(screen.getByText('MEDIAN FEE-RATE')).toBeInTheDocument();

    await user.click(screen.getByText('CIRCLES'));

    // Back to circles: the selected-cluster panel is restored.
    expect(panelLabel()).toBe('Selected cluster panel');
    expect(screen.getByText('TXIDS')).toBeInTheDocument();
    expect(screen.queryByText('5 CLUSTERS')).not.toBeInTheDocument();
  });
});
