import { sim } from '../lib/sim';
import { PreviewSparkline } from './PreviewSparkline';

export interface FeerateDiagramPreviewProps {
  width: number;
  height: number;
}

/** Decorative feerate-diagram growth-curve shape for the home preview card. */
export function FeerateDiagramPreview({
  width,
  height,
}: FeerateDiagramPreviewProps) {
  const { line, area } = sim.feerateDiagramAt(width, height);

  return (
    <PreviewSparkline
      width={width}
      height={height}
      line={line}
      area={area}
      title="Mempool feerate diagram preview"
    />
  );
}
