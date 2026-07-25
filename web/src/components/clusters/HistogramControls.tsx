import type { ClusterMetric } from '../../lib/clusterMetrics';
import { Select } from '../Select';
import { Slider } from '../Slider';
import { METRIC_OPTIONS } from './MetricSelects';

const MIN_BINS = 5;
const MAX_BINS = 50;

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
        min={MIN_BINS}
        max={MAX_BINS}
        onChange={onBinsChange}
      />
    </div>
  );
}
