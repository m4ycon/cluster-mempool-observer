import { Pause, Play } from 'lucide-react';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import { SHOW_COUNT_RANGE } from '../../lib/clustersSearch';
import { Divider } from '../Divider';
import { Slider } from '../Slider';
import { VizButton } from '../VizButton';
import type { VizType } from './ClusterCanvas';
import { HistogramControls } from './HistogramControls';
import { MetricSelects } from './MetricSelects';

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
  bins: number;
  onBinsChange: (n: number) => void;
  paused: boolean;
  onTogglePause: () => void;
  linked: boolean;
  onLinkedChange: (linked: boolean) => void;
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
  bins,
  onBinsChange,
  paused,
  onTogglePause,
  linked,
  onLinkedChange,
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
          <VizButton
            active={vizType === 'histogram'}
            onClick={() => onVizTypeChange('histogram')}
          >
            HISTOGRAM
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
        {vizType === 'histogram' ? (
          <HistogramControls
            sizeMetric={sizeMetric}
            onSizeMetricChange={onSizeMetricChange}
            bins={bins}
            onBinsChange={onBinsChange}
          />
        ) : (
          <>
            <MetricSelects
              sizeMetric={sizeMetric}
              onSizeMetricChange={onSizeMetricChange}
              colorMetric={colorMetric}
              onColorMetricChange={onColorMetricChange}
              linked={linked}
              onLinkedChange={onLinkedChange}
            />

            <Divider orientation="vertical" className="" />

            <Slider
              label="SHOW"
              value={showCount}
              min={SHOW_COUNT_RANGE.min}
              max={SHOW_COUNT_RANGE.max}
              onChange={onShowCountChange}
            />
          </>
        )}
      </div>
    </div>
  );
}
