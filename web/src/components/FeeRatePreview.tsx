import { sim } from '../lib/sim';
import { PreviewSparkline } from './PreviewSparkline';

export interface FeeRatePreviewProps {
  width: number;
  height: number;
}

/** Decorative median fee-rate shape for the home preview card. */
export function FeeRatePreview({ width, height }: FeeRatePreviewProps) {
  const { med, area } = sim.feeAt('24h', width, height);

  return (
    <PreviewSparkline
      width={width}
      height={height}
      line={med}
      area={area}
      title="Fee-rate preview"
    />
  );
}
