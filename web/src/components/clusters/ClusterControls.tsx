import { Pause, Play } from 'lucide-react';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { Divider } from '../Divider';
import { Slider } from '../Slider';
import { VizButton } from '../VizButton';
import type { VizType } from './ClusterCanvas';
import { MetricSelects } from './MetricSelects';

const MIN_SHOW_COUNT = 10;
const MAX_SHOW_COUNT = 250;

const ICON_SIZE = 14;

export interface ClusterControlsProps {
  vizType: VizType;
  onVizTypeChange: (v: VizType) => void;
  sizeMetric: ClusterMetric;
  onSizeMetricChange: (m: ClusterMetric) => void;
  colorMetric: ClusterMetric;
  onColorMetricChange: (m: ClusterMetric) => void;
  showCount: number;
  onShowCountChange: (n: number) => void;
  paused: boolean;
  onTogglePause: () => void;
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
  paused,
  onTogglePause,
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

        <VizButton
          active={paused}
          onClick={onTogglePause}
          ariaLabel={paused ? 'Resume live stream' : 'Pause live stream'}
        >
          {paused ? (
            <Play size={ICON_SIZE} aria-hidden="true" />
          ) : (
            <Pause size={ICON_SIZE} aria-hidden="true" />
          )}
        </VizButton>
      </div>

      <div className="flex flex-wrap items-center gap-4">
        <MetricSelects
          sizeMetric={sizeMetric}
          onSizeMetricChange={onSizeMetricChange}
          colorMetric={colorMetric}
          onColorMetricChange={onColorMetricChange}
        />

        <Divider orientation="vertical" className="" />

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
