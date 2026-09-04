import type { ClusterMetric } from '../../lib/clusterMetrics';
import { Select, type SelectOption } from '../Select';
import { VizButton } from '../VizButton';

export const METRIC_OPTIONS: readonly SelectOption<ClusterMetric>[] = [
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
  linked: boolean;
  onLinkedChange: (linked: boolean) => void;
}

export function MetricSelects({
  sizeMetric,
  onSizeMetricChange,
  colorMetric,
  onColorMetricChange,
  linked,
  onLinkedChange,
}: MetricSelectsProps) {
  const toggle = () => {
    if (!linked) onColorMetricChange(sizeMetric);
    onLinkedChange(!linked);
  };

  const DetailMetricButton = () => (
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
  );

  return (
    <div className="flex flex-col items-start gap-3">
      {linked ? (
        <div className="flex items-end gap-3">
          <Select
            label="SIZE/COLOR BY"
            value={sizeMetric}
            options={METRIC_OPTIONS}
            onChange={onSizeMetricChange}
            orientation="vertical"
          />
          <DetailMetricButton />
        </div>
      ) : (
        <>
          <div className="flex items-end gap-3">
            <Select
              label="SIZE BY"
              value={sizeMetric}
              options={METRIC_OPTIONS}
              onChange={onSizeMetricChange}
              orientation="vertical"
            />
            <DetailMetricButton />
          </div>
          <Select
            label="COLOR BY"
            value={colorMetric}
            options={METRIC_OPTIONS}
            onChange={onColorMetricChange}
            orientation="vertical"
          />
        </>
      )}
    </div>
  );
}
