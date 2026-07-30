import type { ClusterMetric } from '../../lib/clusterMetrics';
import { BINS_RANGE } from '../../lib/clustersSearch';
import { Select } from '../Select';
import { Slider } from '../Slider';
import { METRIC_OPTIONS } from './MetricSelects';

export interface HistogramControlsProps {
  sizeMetric: ClusterMetric;
  onSizeMetricChange: (m: ClusterMetric) => void;
  bins: number;
  onBinsChange: (n: number) => void;
}

export function HistogramControls({
  sizeMetric,
  onSizeMetricChange,
  bins,
  onBinsChange,
}: HistogramControlsProps) {
  return (
    <div className="flex flex-wrap items-center gap-4">
      <Select
        label="BIN BY"
        value={sizeMetric}
        options={METRIC_OPTIONS}
        onChange={onSizeMetricChange}
      />

      <Slider
        label="BINS"
        value={bins}
        min={BINS_RANGE.min}
        max={BINS_RANGE.max}
        onChange={onBinsChange}
      />
    </div>
  );
}
