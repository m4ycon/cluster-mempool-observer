use crate::TS_EXPORT_DIR;
use crate::api::FeerateDiagramPoint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Basically a cluster's chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagramSegment {
    pub fee_sats: i64,
    pub weight: u64,
}

impl DiagramSegment {
    /// Splits a cumulative diagram back into one segment per chunk, each with
    /// the index of its point.
    fn from_points(points: &[FeerateDiagramPoint]) -> Vec<(u32, Self)> {
        let mut previous = FeerateDiagramPoint {
            weight: 0,
            fee_sats: 0,
        };
        let mut segments = Vec::with_capacity(points.len());
        for (index, point) in (0..).zip(points) {
            let weight = point.weight.saturating_sub(previous.weight);
            if weight > 0 {
                let segment = Self {
                    fee_sats: point.fee_sats - previous.fee_sats,
                    weight,
                };
                segments.push((index, segment));
            }
            previous = point.clone();
        }
        segments
    }

    /// The cumulative diagram of `segments`, highest feerate first, from the
    /// origin, the same shape the node's diagram comes in.
    pub fn to_points(segments: &[Self]) -> Vec<FeerateDiagramPoint> {
        let mut sorted = segments.to_vec();
        sorted.sort_by(|a, b| {
            (i128::from(b.fee_sats) * i128::from(a.weight))
                .cmp(&(i128::from(a.fee_sats) * i128::from(b.weight)))
        });
        let mut point = FeerateDiagramPoint {
            weight: 0,
            fee_sats: 0,
        };
        let mut points = Vec::with_capacity(sorted.len() + 1);
        points.push(point.clone());
        for segment in sorted {
            point.weight += segment.weight;
            point.fee_sats += segment.fee_sats;
            points.push(point.clone());
        }
        points
    }
}

/// What missing, extra and the node total count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum FeerateDiagramDiffUnit {
    ChunkCount,
    Weight,
}

impl FeerateDiagramDiffUnit {
    fn of(self, segment: &DiagramSegment) -> u64 {
        match self {
            Self::ChunkCount => 1,
            Self::Weight => segment.weight,
        }
    }

    fn total(self, segments: &[(u32, DiagramSegment)]) -> u64 {
        segments.iter().map(|(_, segment)| self.of(segment)).sum()
    }
}

#[derive(Debug, Clone)]
pub struct DiagramComparison {
    node: Vec<(u32, DiagramSegment)>,
    ours_points: Vec<FeerateDiagramPoint>,
    missing: Vec<(u32, DiagramSegment)>,
    extra: Vec<(u32, DiagramSegment)>,
}

impl DiagramComparison {
    pub fn new(node_points: &[FeerateDiagramPoint], ours: &[DiagramSegment]) -> Self {
        let node = DiagramSegment::from_points(node_points);
        let ours_points = DiagramSegment::to_points(ours);
        let ours = DiagramSegment::from_points(&ours_points);

        // Equal (fee, weight) implies equal feerate, so matching segments needs
        // no walk in feerate order, and order inside a feerate tie is moot.
        let mut balance: HashMap<DiagramSegment, i64> = HashMap::new();
        for (_, segment) in &node {
            *balance.entry(*segment).or_default() += 1;
        }
        for (_, segment) in &ours {
            *balance.entry(*segment).or_default() -= 1;
        }

        let mut node_surplus = HashMap::new();
        let mut ours_surplus = HashMap::new();
        for (segment, count) in balance {
            if count > 0 {
                node_surplus.insert(segment, count.unsigned_abs());
            } else if count < 0 {
                ours_surplus.insert(segment, count.unsigned_abs());
            }
        }

        Self {
            missing: take_surplus(&node, node_surplus),
            extra: take_surplus(&ours, ours_surplus),
            node,
            ours_points,
        }
    }

    /// What the node has that we lack.
    pub fn missing(&self, unit: FeerateDiagramDiffUnit) -> u64 {
        unit.total(&self.missing)
    }

    /// What we have that the node lacks.
    pub fn extra(&self, unit: FeerateDiagramDiffUnit) -> u64 {
        unit.total(&self.extra)
    }

    pub fn node_total(&self, unit: FeerateDiagramDiffUnit) -> u64 {
        unit.total(&self.node)
    }

    pub fn ours_points(&self) -> &[FeerateDiagramPoint] {
        &self.ours_points
    }

    /// Index, in the node's points, of each missing chunk.
    pub fn missing_chunks(&self) -> Vec<u32> {
        self.missing.iter().map(|(index, _)| *index).collect()
    }

    /// Index, in `ours_points`, of each extra chunk.
    pub fn extra_chunks(&self) -> Vec<u32> {
        self.extra.iter().map(|(index, _)| *index).collect()
    }

    /// `amount` (missing or extra) over the node's total, so extra can pass 1.
    /// `None` for an empty node diagram.
    pub fn share_of_node(amount: u64, node_total: u64) -> Option<f64> {
        (node_total > 0).then(|| amount as f64 / node_total as f64)
    }
}

/// Takes each surplus segment where it first appears on `side`. Copies of a
/// chunk can't be told apart, so any of them is the missing or extra one.
fn take_surplus(
    side: &[(u32, DiagramSegment)],
    mut surplus: HashMap<DiagramSegment, u64>,
) -> Vec<(u32, DiagramSegment)> {
    side.iter()
        .filter(|(_, segment)| match surplus.get_mut(segment) {
            Some(left) if *left > 0 => {
                *left -= 1;
                true
            }
            _ => false,
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(fee_sats: i64, weight: u64) -> DiagramSegment {
        DiagramSegment { fee_sats, weight }
    }

    fn point(weight: u64, fee_sats: i64) -> FeerateDiagramPoint {
        FeerateDiagramPoint { weight, fee_sats }
    }

    fn compare(node: &[DiagramSegment], ours: &[DiagramSegment]) -> DiagramComparison {
        DiagramComparison::new(&DiagramSegment::to_points(node), ours)
    }

    /// A small diagram with a long feerate tail, highest feerate first.
    fn node_diagram() -> Vec<DiagramSegment> {
        vec![
            seg(200_000, 400),
            seg(20_000, 800),
            seg(6_000, 1_200),
            seg(3_000, 2_000),
            seg(1_500, 1_000),
            seg(1_000, 4_000),
            seg(1_000, 4_000),
            seg(400, 1_600),
        ]
    }

    #[test]
    fn from_points_splits_the_cumulative_diagram_into_chunks() {
        let points = [point(0, 0), point(400, 2_000), point(1_200, 3_000)];

        assert_eq!(
            DiagramSegment::from_points(&points),
            vec![(1, seg(2_000, 400)), (2, seg(1_000, 800))]
        );
    }

    #[test]
    fn from_points_without_an_origin_starts_from_zero() {
        let points = [point(400, 2_000), point(1_200, 3_000)];

        assert_eq!(
            DiagramSegment::from_points(&points),
            vec![(0, seg(2_000, 400)), (1, seg(1_000, 800))]
        );
    }

    #[test]
    fn to_points_stacks_segments_by_falling_feerate_from_the_origin() {
        let segments = [seg(1_000, 800), seg(2_000, 400), seg(300, 400)];

        assert_eq!(
            DiagramSegment::to_points(&segments),
            vec![
                point(0, 0),
                point(400, 2_000),
                point(1_200, 3_000),
                point(1_600, 3_300)
            ]
        );
    }

    #[test]
    fn to_points_undoes_from_points() {
        let points = DiagramSegment::to_points(&node_diagram());

        assert_eq!(
            DiagramSegment::to_points(
                &DiagramSegment::from_points(&points)
                    .into_iter()
                    .map(|(_, segment)| segment)
                    .collect::<Vec<_>>()
            ),
            points
        );
    }

    #[test]
    fn to_points_of_nothing_is_the_origin() {
        assert_eq!(DiagramSegment::to_points(&[]), vec![point(0, 0)]);
    }

    #[test]
    fn identical_diagrams_have_no_difference() {
        let comparison = compare(&node_diagram(), &node_diagram());

        for unit in [
            FeerateDiagramDiffUnit::Weight,
            FeerateDiagramDiffUnit::ChunkCount,
        ] {
            assert_eq!(comparison.missing(unit), 0);
            assert_eq!(comparison.extra(unit), 0);
        }
    }

    #[test]
    fn node_total_counts_the_whole_node_diagram() {
        let comparison = compare(&node_diagram(), &[]);

        assert_eq!(
            comparison.node_total(FeerateDiagramDiffUnit::Weight),
            15_000
        );
        assert_eq!(comparison.node_total(FeerateDiagramDiffUnit::ChunkCount), 8);
    }

    #[test]
    fn an_island_counts_only_itself() {
        let node = node_diagram();
        let mut ours = node.clone();
        ours[3] = seg(3_001, 2_000);

        let comparison = compare(&node, &ours);

        assert_eq!(comparison.missing(FeerateDiagramDiffUnit::Weight), 2_000);
        assert_eq!(comparison.extra(FeerateDiagramDiffUnit::Weight), 2_000);
        assert_eq!(comparison.missing(FeerateDiagramDiffUnit::ChunkCount), 1);
        assert_eq!(comparison.extra(FeerateDiagramDiffUnit::ChunkCount), 1);
    }

    #[test]
    fn order_inside_a_feerate_tie_does_not_matter() {
        let node = vec![seg(1_000, 400), seg(2_000, 800), seg(100, 400)];
        let ours = vec![seg(2_000, 800), seg(1_000, 400), seg(100, 400)];

        let comparison = compare(&node, &ours);

        assert_eq!(comparison.missing(FeerateDiagramDiffUnit::Weight), 0);
        assert_eq!(comparison.extra(FeerateDiagramDiffUnit::Weight), 0);
    }

    #[test]
    fn repeated_chunks_match_one_for_one() {
        let node = vec![seg(1_000, 400), seg(1_000, 400), seg(1_000, 400)];
        let ours = vec![seg(1_000, 400), seg(1_000, 400)];

        let comparison = compare(&node, &ours);

        assert_eq!(comparison.missing(FeerateDiagramDiffUnit::ChunkCount), 1);
        assert_eq!(comparison.extra(FeerateDiagramDiffUnit::ChunkCount), 0);

        let comparison = compare(&ours, &node);

        assert_eq!(comparison.missing(FeerateDiagramDiffUnit::ChunkCount), 0);
        assert_eq!(comparison.extra(FeerateDiagramDiffUnit::ChunkCount), 1);
    }

    #[test]
    fn an_empty_side_of_ours_misses_everything() {
        let comparison = compare(&node_diagram(), &[]);

        assert_eq!(comparison.missing(FeerateDiagramDiffUnit::Weight), 15_000);
        assert_eq!(comparison.extra(FeerateDiagramDiffUnit::Weight), 0);
    }

    #[test]
    fn share_of_node_is_over_the_node_total_so_extra_can_pass_one() {
        assert_eq!(DiagramComparison::share_of_node(1, 4), Some(0.25));
        assert_eq!(DiagramComparison::share_of_node(6, 4), Some(1.5));
    }

    #[test]
    fn an_empty_node_diagram_gives_no_share() {
        let comparison = compare(&[], &node_diagram());
        let unit = FeerateDiagramDiffUnit::ChunkCount;

        assert_eq!(comparison.node_total(unit), 0);
        assert_eq!(
            DiagramComparison::share_of_node(comparison.extra(unit), comparison.node_total(unit)),
            None
        );
        assert_eq!(DiagramComparison::share_of_node(0, 0), None);
    }

    #[test]
    fn an_island_is_reported_by_its_point_on_each_curve() {
        let node = node_diagram();
        let mut ours = node.clone();
        ours[3] = seg(3_001, 2_000);

        let comparison = compare(&node, &ours);

        // Chunk 3 is point 4: the points start at the origin.
        assert_eq!(comparison.missing_chunks(), [4]);
        assert_eq!(comparison.extra_chunks(), [4]);
    }

    #[test]
    fn a_repeated_chunk_short_on_one_side_is_placed_once() {
        let node = vec![
            seg(5_000, 400),
            seg(1_000, 400),
            seg(1_000, 400),
            seg(1_000, 400),
        ];
        let ours = vec![seg(5_000, 400), seg(1_000, 400), seg(1_000, 400)];

        let comparison = compare(&node, &ours);

        assert_eq!(comparison.missing_chunks(), [2]);
        assert!(comparison.extra_chunks().is_empty());
    }

    #[test]
    fn every_node_chunk_is_missing_when_ours_is_empty() {
        let node = vec![seg(2_000, 400), seg(1_000, 800)];

        let comparison = compare(&node, &[]);

        assert_eq!(comparison.missing_chunks(), [1, 2]);
        assert!(comparison.extra_chunks().is_empty());
    }

    #[test]
    fn extra_chunks_index_our_curve_stacked_by_feerate() {
        let node = vec![seg(2_000, 400)];
        let ours = vec![seg(300, 400), seg(2_000, 400)];

        let comparison = compare(&node, &ours);

        assert_eq!(
            comparison.ours_points(),
            [point(0, 0), point(400, 2_000), point(800, 2_300)]
        );
        assert_eq!(comparison.extra_chunks(), [2]);
    }
}
