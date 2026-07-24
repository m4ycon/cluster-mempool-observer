import type { ClusterMetric } from '../../lib/clusterMetrics';
import { Select, type SelectOption } from '../Select';
import { Slider } from '../Slider';
import { VizButton } from '../VizButton';
import type { VizType } from './ClusterCanvas';

const MIN_SHOW_COUNT = 10;
const MAX_SHOW_COUNT = 250;

const METRIC_OPTIONS: readonly SelectOption<ClusterMetric>[] = [
  { value: 'vsize', label: 'VSIZE' },
  { value: 'fee', label: 'TOTAL FEE' },
  { value: 'feerate', label: 'SAT/VB' },
  { value: 'txs', label: 'TX COUNT' },
];

export interface ClusterControlsProps {
  vizType: VizType;
  onVizTypeChange: (v: VizType) => void;
  sizeMetric: ClusterMetric;
  onSizeMetricChange: (m: ClusterMetric) => void;
  colorMetric: ClusterMetric;
  onColorMetricChange: (m: ClusterMetric) => void;
  showCount: number;
  onShowCountChange: (n: number) => void;
}

export function ClusterControls({
  vizType,
  onVizTypeChange,
  sizeMetric,
  onSizeMetricChange,
  colorMetric,
  onColorMetricChange,
  showCount,
  onShowCountChange,
}: ClusterControlsProps) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-4 border-line border-b px-6 py-2">
      <div className="flex items-center gap-2">
        <span className="text-xs text-dim tracking-[0.12em]">VIZ</span>
        <div className="flex gap-1">
          <VizButton
            active={vizType === 'circles'}
            onClick={() => onVizTypeChange('circles')}
          >
            CIRCLES
          </VizButton>
          <VizButton
            active={vizType === 'treemap'}
            onClick={() => onVizTypeChange('treemap')}
          >
            TREEMAP
          </VizButton>
        </div>
      </div>

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

        <Slider
          label="SHOW"
          value={showCount}
          min={MIN_SHOW_COUNT}
          max={MAX_SHOW_COUNT}
          onChange={onShowCountChange}
        />
      </div>
    </div>
  );
}
