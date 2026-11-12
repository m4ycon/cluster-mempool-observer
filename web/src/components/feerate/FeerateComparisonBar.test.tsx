import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { TimeFormat } from '../../lib/format';
import type { FeerateDiagramComparison } from '../../types/generated/FeerateDiagramComparison';
import { FeerateComparisonBar } from './FeerateComparisonBar';

function comparison(
  overrides: Partial<FeerateDiagramComparison> = {},
): FeerateDiagramComparison {
  return {
    sampled_at: '2026-08-12T12:04:31Z',
    node: [],
    ours: [],
    missing: 37,
    extra: 5,
    node_total: 19_022,
    unit: 'chunk_count',
    missing_chunks: [],
    extra_chunks: [],
    ...overrides,
  };
}

describe('FeerateComparisonBar', () => {
  it('shows missing, extra and the node total as absolute counts', () => {
    render(<FeerateComparisonBar comparison={comparison()} />);

    expect(screen.getByText('37')).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();
    expect(screen.getByText('19,022')).toBeInTheDocument();
    expect(
      screen.getByText('node chunks', { exact: false }),
    ).toBeInTheDocument();
  });

  it('names the node total in the unit the comparison counted', () => {
    render(
      <FeerateComparisonBar
        comparison={comparison({ unit: 'weight', node_total: 80_000_000 })}
      />,
    );

    expect(screen.getByText('80,000,000')).toBeInTheDocument();
    expect(screen.getByText('node WU', { exact: false })).toBeInTheDocument();
  });

  it('says when the pair was sampled and how long ago', () => {
    const c = comparison();
    render(<FeerateComparisonBar comparison={c} />);

    expect(
      screen.getByText(
        `sampled ${TimeFormat.at(c.sampled_at)} · ${TimeFormat.ago(c.sampled_at)} ago`,
      ),
    ).toBeInTheDocument();
  });
});
