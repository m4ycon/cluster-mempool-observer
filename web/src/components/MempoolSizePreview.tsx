import { sim } from '../lib/sim';
import { PreviewSparkline } from './PreviewSparkline';

export interface MempoolSizePreviewProps {
  width: number;
  height: number;
}

/** Decorative mempool-size-over-time shape for the home preview card. */
export function MempoolSizePreview({ width, height }: MempoolSizePreviewProps) {
  const { line, area } = sim.mempoolSizeAt(width, height);

  return (
    <PreviewSparkline
      width={width}
      height={height}
      line={line}
      area={area}
      title="Mempool size preview"
    />
  );
}
