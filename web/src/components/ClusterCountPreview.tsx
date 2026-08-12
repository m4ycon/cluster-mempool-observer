import { sim } from '../lib/sim';
import { PreviewSparkline } from './PreviewSparkline';

export interface ClusterCountPreviewProps {
  width: number;
  height: number;
}

/** Decorative cluster-count-over-time shape for the home preview card. */
export function ClusterCountPreview({
  width,
  height,
}: ClusterCountPreviewProps) {
  const { line, area } = sim.clusterCountAt(width, height);

  return (
    <PreviewSparkline
      width={width}
      height={height}
      line={line}
      area={area}
      title="Cluster count preview"
    />
  );
}
