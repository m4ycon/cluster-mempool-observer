import { hierarchy, pack, treemap, treemapSquarify } from 'd3-hierarchy';
import type { ClusterRef } from '../types/events';
import type { ClusterMetric } from './clusterMetrics';
import { ClusterMetrics } from './clusterMetrics';

interface ClusterTree {
  children: ClusterRef[];
}

export interface PackedCluster {
  c: ClusterRef;
  x: number;
  y: number;
  r: number;
}

export interface TreemapCell {
  c: ClusterRef;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Packs clusters into circles sized by `sizeMetric`, viewBox `W x H`. */
export function packLayout(
  clusters: ClusterRef[],
  sizeMetric: ClusterMetric,
  W = 720,
  H = 560,
): PackedCluster[] {
  if (clusters.length === 0) return [];

  const root = hierarchy<ClusterTree | ClusterRef>({
    children: clusters,
  } as ClusterTree)
    .sum((d) =>
      'id' in d
        ? Math.max(0.01, ClusterMetrics.value(d as ClusterRef, sizeMetric))
        : 0,
    )
    .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

  const laidOut = pack<ClusterTree | ClusterRef>().size([W, H]).padding(3)(
    root,
  );

  return laidOut.leaves().map((leaf) => ({
    c: leaf.data as ClusterRef,
    x: leaf.x,
    y: leaf.y,
    r: leaf.r,
  }));
}

/** Tiles clusters into rects sized by `sizeMetric`, viewBox `W x H`. */
export function treemapLayout(
  clusters: ClusterRef[],
  sizeMetric: ClusterMetric,
  W = 720,
  H = 560,
): TreemapCell[] {
  if (clusters.length === 0) return [];

  const root = hierarchy<ClusterTree | ClusterRef>({
    children: clusters,
  } as ClusterTree)
    .sum((d) =>
      'id' in d
        ? Math.max(0.01, ClusterMetrics.value(d as ClusterRef, sizeMetric))
        : 0,
    )
    .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

  const laidOut = treemap<ClusterTree | ClusterRef>()
    .size([W, H])
    .paddingInner(2)
    .tile(treemapSquarify)(root);

  return laidOut.leaves().map((leaf) => ({
    c: leaf.data as ClusterRef,
    x0: leaf.x0,
    y0: leaf.y0,
    x1: leaf.x1,
    y1: leaf.y1,
  }));
}
