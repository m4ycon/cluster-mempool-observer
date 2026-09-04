import { Filter, Pause, Play } from 'lucide-react';
import type { ReadyState } from 'react-use-websocket';
import type { ClusterMetric } from '../../lib/clusterMetrics';
import type { VizType } from '../../lib/clustersSearch';
import { SHOW_COUNT_RANGE } from '../../lib/clustersSearch';
import { ConnectionDot } from '../ConnectionDot';
import { Divider } from '../Divider';
import { HelpButton } from '../help/HelpButton';
import type { HelpTopic } from '../help/panelHelp';
import { Popover } from '../Popover';
import { Slider } from '../Slider';
import { Tooltip } from '../Tooltip';
import { VizButton } from '../VizButton';
import { HistogramControls } from './HistogramControls';
import { MetricSelects } from './MetricSelects';

const ICON_SIZE = 14;

const VIZ_OPTIONS: readonly {
  type: VizType;
  label: string;
  topic: HelpTopic;
}[] = [
  { type: 'circles', label: 'CIRCLES', topic: 'clusters.circles' },
  { type: 'treemap', label: 'TREEMAP', topic: 'clusters.treemap' },
  { type: 'histogram', label: 'HISTOGRAM', topic: 'clusters.histogram' },
  { type: 'table', label: 'TABLE', topic: 'clusters.table' },
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
  bins: number;
  onBinsChange: (n: number) => void;
  paused: boolean;
  onTogglePause: () => void;
  readyState: ReadyState;
  linked: boolean;
  onLinkedChange: (linked: boolean) => void;
  includeSingletons: boolean;
  onIncludeSingletonsChange: (include: boolean) => void;
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
  readyState,
  linked,
  onLinkedChange,
  includeSingletons,
  onIncludeSingletonsChange,
}: ClusterControlsProps) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-4 border-line border-b px-6 py-2">
      <div className="flex items-center gap-2">
        <span className="text-xs text-dim tracking-[0.12em]">VIZ</span>
        <div className="flex items-center gap-1">
          {VIZ_OPTIONS.map(({ type, label, topic }) => (
            <span key={type} className="flex items-center gap-1">
              <VizButton
                active={vizType === type}
                onClick={() => onVizTypeChange(type)}
              >
                {label}
              </VizButton>
              {vizType === type && <HelpButton topic={topic} />}
            </span>
          ))}
        </div>
      </div>

      <div className="flex items-center gap-3">
        <ConnectionDot
          readyState={readyState}
          label="CLUSTER FEED"
          align="right"
        />

        <Tooltip
          label={paused ? 'RESUME LIVE FEED' : 'FREEZE LIVE FEED'}
          align="right"
        >
          <VizButton
            active={paused}
            variant="alert"
            onClick={onTogglePause}
            ariaLabel={paused ? 'Resume live stream' : 'Pause live stream'}
          >
            {paused ? (
              <Play size={ICON_SIZE} aria-hidden="true" />
            ) : (
              <Pause size={ICON_SIZE} aria-hidden="true" />
            )}
          </VizButton>
        </Tooltip>

        <Popover
          trigger={({ open, toggle }) => (
            <VizButton
              active={open}
              onClick={toggle}
              ariaLabel="Toggle filters"
            >
              <span className="flex items-center gap-1.5">
                <Filter size={ICON_SIZE} aria-hidden="true" />
                FILTERS
              </span>
            </VizButton>
          )}
        >
          {vizType === 'histogram' && (
            <HistogramControls
              sizeMetric={sizeMetric}
              onSizeMetricChange={onSizeMetricChange}
              bins={bins}
              onBinsChange={onBinsChange}
            />
          )}

          {(vizType === 'circles' || vizType === 'treemap') && (
            <>
              <MetricSelects
                sizeMetric={sizeMetric}
                onSizeMetricChange={onSizeMetricChange}
                colorMetric={colorMetric}
                onColorMetricChange={onColorMetricChange}
                linked={linked}
                onLinkedChange={onLinkedChange}
              />

              <Divider />

              <Slider
                label="SHOW"
                value={showCount}
                min={SHOW_COUNT_RANGE.min}
                max={SHOW_COUNT_RANGE.max}
                onChange={onShowCountChange}
                orientation="vertical"
              />
            </>
          )}

          {vizType !== 'table' && <Divider />}

          <Tooltip
            label={
              includeSingletons
                ? 'HIDE SINGLE-TX CLUSTERS'
                : 'SHOW SINGLE-TX CLUSTERS'
            }
          >
            <VizButton
              active={includeSingletons}
              onClick={() => onIncludeSingletonsChange(!includeSingletons)}
              ariaLabel={
                includeSingletons
                  ? 'Exclude singleton clusters'
                  : 'Include singleton clusters'
              }
            >
              SINGLETONS
            </VizButton>
          </Tooltip>
        </Popover>
      </div>
    </div>
  );
}
