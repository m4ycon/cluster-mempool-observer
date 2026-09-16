import { sim } from '../lib/sim';
import { PreviewMultiSparkline } from './PreviewMultiSparkline';

export interface TxsPerMinPreviewProps {
  width: number;
  height: number;
}

/** Decorative txs/min-over-time shape for the home preview card: arrivals, confirmed, evicted. */
export function TxsPerMinPreview({ width, height }: TxsPerMinPreviewProps) {
  const { added, confirmed, evicted } = sim.txsPerMinAt(width, height);

  return (
    <PreviewMultiSparkline
      width={width}
      height={height}
      title="Txs/min preview"
      lines={[
        { path: added, color: '#3fb950' },
        { path: confirmed, color: '#f7931a' },
        { path: evicted, color: '#d9544d' },
      ]}
    />
  );
}
