import { useState } from 'react';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { Select, type SelectOption } from '../Select';
import { VizButton } from '../VizButton';

const METRIC_OPTIONS: readonly SelectOption<ClusterMetric>[] = [
  { value: 'feerate', label: 'SAT/VB' },
  { value: 'txs', label: 'TX COUNT' },
  { value: 'vsize', label: 'TOTAL VSIZE' },
  { value: 'fee', label: 'TOTAL FEE' },
];

export interface MetricSelectsProps {
  sizeMetric: ClusterMetric;
  onSizeMetricChange: (m: ClusterMetric) => void;
  colorMetric: ClusterMetric;
  onColorMetricChange: (m: ClusterMetric) => void;
}

export function MetricSelects({
  sizeMetric,
  onSizeMetricChange,
  colorMetric,
  onColorMetricChange,
}: MetricSelectsProps) {
  const [linked, setLinked] = useState(true);

  const toggle = () => {
    if (!linked) onColorMetricChange(sizeMetric);
    setLinked(!linked);
  };

  const changeBoth = (m: ClusterMetric) => {
    onSizeMetricChange(m);
    onColorMetricChange(m);
  };

  return (
    <div className="flex items-center gap-2">
      {linked ? (
        <Select
          label="SIZE/COLOR BY"
          value={sizeMetric}
          options={METRIC_OPTIONS}
          onChange={changeBoth}
        />
      ) : (
        <div className="flex items-center gap-4">
          <Select
            label="SIZE BY"
            value={sizeMetric}
            options={METRIC_OPTIONS}
            onChange={onSizeMetricChange}
          />
          <Select
            label="COLOR BY"
            value={colorMetric}
            options={METRIC_OPTIONS}
            onChange={onColorMetricChange}
          />
        </div>
      )}

      <VizButton
        active={linked}
        onClick={toggle}
        ariaLabel={
          linked
            ? 'Size and colour by separate metrics'
            : 'Size and colour by one metric'
        }
      >
        {linked ? '+' : '-'}
      </VizButton>
    </div>
  );
}
