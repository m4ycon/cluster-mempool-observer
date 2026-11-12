import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { GaugePoint } from '../../types/generated/GaugePoint';
import type { GaugeSeries } from '../../types/generated/GaugeSeries';
import { GaugePercentiles } from './GaugePercentiles';

function points(values: (number | null)[]): GaugePoint[] {
  return values.map((value, i) => ({
    sampled_at: new Date(Date.UTC(2026, 6, 1, 12, i)).toISOString(),
    value,
  }));
}

function series(
  metric: GaugeSeries['metric'],
  values: (number | null)[],
): GaugeSeries {
  return { metric, resolution_secs: 60, points: points(values) };
}

describe('GaugePercentiles', () => {
  it('interpolates each percentile and skips samples without a reading', () => {
    const shares = Array.from({ length: 21 }, (_, i) => i / 100);
    render(
      <GaugePercentiles
        series={[
          series('feerate-diagram-missing-percentage', [null, ...shares, null]),
        ]}
        percentiles={[95, 99]}
      />,
    );

    expect(screen.getByTestId('gauge-percentiles')).toHaveTextContent(
      'missingp95 19%p99 19.8%',
    );
  });

  it('shows a dash for a series with no reading in the window', () => {
    render(
      <GaugePercentiles
        series={[series('feerate-diagram-extra-percentage', [null, null])]}
        percentiles={[95]}
      />,
    );

    expect(screen.getByTestId('gauge-percentiles')).toHaveTextContent(
      'extrap95 --',
    );
  });

  it('lists the series in the order given', () => {
    render(
      <GaugePercentiles
        series={[
          series('feerate-diagram-extra-percentage', [0.5]),
          series('feerate-diagram-missing-percentage', [0.25]),
        ]}
        percentiles={[95]}
      />,
    );

    expect(screen.getByTestId('gauge-percentiles')).toHaveTextContent(
      'extrap95 50%missingp95 25%',
    );
  });
});
