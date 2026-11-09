use crate::api::FeerateDiagramPoint;
use std::collections::HashMap;

/// Basically a cluster's chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagramSegment {
    pub fee_sats: i64,
    pub weight: u64,
}

impl DiagramSegment {
    /// Splits a cumulative diagram back into one segment per chunk.
    pub fn from_points(points: &[FeerateDiagramPoint]) -> Vec<Self> {
        let mut previous = FeerateDiagramPoint {
            weight: 0,
            fee_sats: 0,
        };
        let mut segments = Vec::with_capacity(points.len());
        for point in points {
            let weight = point.weight.saturating_sub(previous.weight);
            if weight > 0 {
                segments.push(Self {
                    fee_sats: point.fee_sats - previous.fee_sats,
                    weight,
                });
            }
            previous = point.clone();
        }
        segments
    }
}

/// What a missing or extra percentage counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FeerateDiagramDiffUnit {
    #[default]
    Weight,
    ChunkCount,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Amount {
    weight: u64,
    chunks: u64,
}

impl Amount {
    fn add(&mut self, segment: &DiagramSegment, count: u64) {
        self.weight += segment.weight * count;
        self.chunks += count;
    }

    fn in_unit(self, unit: FeerateDiagramDiffUnit) -> u64 {
        match unit {
            FeerateDiagramDiffUnit::Weight => self.weight,
            FeerateDiagramDiffUnit::ChunkCount => self.chunks,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiagramComparison {
    node_total: Amount,
    missing: Amount,
    extra: Amount,
}

impl DiagramComparison {
    pub fn new(node: &[DiagramSegment], ours: &[DiagramSegment]) -> Self {
        // Equal (fee, weight) implies equal feerate, so matching segments needs
        // no walk in feerate order, and order inside a feerate tie is moot.
        let mut balance: HashMap<DiagramSegment, i64> = HashMap::new();
        let mut node_total = Amount::default();
        for segment in node.iter().filter(|s| s.weight > 0) {
            *balance.entry(*segment).or_default() += 1;
            node_total.add(segment, 1);
        }
        for segment in ours.iter().filter(|s| s.weight > 0) {
            *balance.entry(*segment).or_default() -= 1;
        }

        let mut missing = Amount::default();
        let mut extra = Amount::default();
        for (segment, count) in balance {
            if count > 0 {
                missing.add(&segment, count as u64);
            } else if count < 0 {
                extra.add(&segment, count.unsigned_abs());
            }
        }

        Self {
            node_total,
            missing,
            extra,
        }
    }

    /// Share of the node's diagram we lack, from 0 to 1.
    pub fn missing_percentage(&self, unit: FeerateDiagramDiffUnit) -> Option<f64> {
        self.share_of_node(self.missing, unit)
    }

    /// What we have that the node lacks, over the node's total, so it can pass 1.
    pub fn extra_percentage(&self, unit: FeerateDiagramDiffUnit) -> Option<f64> {
        self.share_of_node(self.extra, unit)
    }

    fn share_of_node(&self, amount: Amount, unit: FeerateDiagramDiffUnit) -> Option<f64> {
        let total = self.node_total.in_unit(unit);
        (total > 0).then(|| amount.in_unit(unit) as f64 / total as f64)
    }
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
        DiagramComparison::new(node, ours)
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
            vec![seg(2_000, 400), seg(1_000, 800)]
        );
    }

    #[test]
    fn from_points_without_an_origin_starts_from_zero() {
        let points = [point(400, 2_000), point(1_200, 3_000)];

        assert_eq!(
            DiagramSegment::from_points(&points),
            vec![seg(2_000, 400), seg(1_000, 800)]
        );
    }

    #[test]
    fn identical_diagrams_have_no_difference() {
        let comparison = compare(&node_diagram(), &node_diagram());

        for unit in [
            FeerateDiagramDiffUnit::Weight,
            FeerateDiagramDiffUnit::ChunkCount,
        ] {
            assert_eq!(comparison.missing_percentage(unit), Some(0.0));
            assert_eq!(comparison.extra_percentage(unit), Some(0.0));
        }
    }

    #[test]
    fn an_island_counts_only_itself() {
        let node = node_diagram();
        let mut ours = node.clone();
        ours[3] = seg(3_001, 2_000);

        let comparison = compare(&node, &ours);

        let node_weight = 15_000.0;
        assert_eq!(
            comparison.missing_percentage(FeerateDiagramDiffUnit::Weight),
            Some(2_000.0 / node_weight)
        );
        assert_eq!(
            comparison.extra_percentage(FeerateDiagramDiffUnit::Weight),
            Some(2_000.0 / node_weight)
        );
        assert_eq!(
            comparison.missing_percentage(FeerateDiagramDiffUnit::ChunkCount),
            Some(1.0 / 8.0)
        );
        assert_eq!(
            comparison.extra_percentage(FeerateDiagramDiffUnit::ChunkCount),
            Some(1.0 / 8.0)
        );
    }

    #[test]
    fn order_inside_a_feerate_tie_does_not_matter() {
        let node = vec![seg(1_000, 400), seg(2_000, 800), seg(100, 400)];
        let ours = vec![seg(2_000, 800), seg(1_000, 400), seg(100, 400)];

        let comparison = compare(&node, &ours);

        assert_eq!(
            comparison.missing_percentage(FeerateDiagramDiffUnit::Weight),
            Some(0.0)
        );
        assert_eq!(
            comparison.extra_percentage(FeerateDiagramDiffUnit::Weight),
            Some(0.0)
        );
    }

    #[test]
    fn repeated_chunks_match_one_for_one() {
        let node = vec![seg(1_000, 400), seg(1_000, 400), seg(1_000, 400)];
        let ours = vec![seg(1_000, 400), seg(1_000, 400)];

        let comparison = compare(&node, &ours);

        assert_eq!(
            comparison.missing_percentage(FeerateDiagramDiffUnit::ChunkCount),
            Some(1.0 / 3.0)
        );
        assert_eq!(
            comparison.extra_percentage(FeerateDiagramDiffUnit::ChunkCount),
            Some(0.0)
        );

        let comparison = compare(&ours, &node);

        assert_eq!(
            comparison.missing_percentage(FeerateDiagramDiffUnit::ChunkCount),
            Some(0.0)
        );
        assert_eq!(
            comparison.extra_percentage(FeerateDiagramDiffUnit::ChunkCount),
            Some(0.5)
        );
    }

    #[test]
    fn an_empty_side_of_ours_misses_everything() {
        let comparison = compare(&node_diagram(), &[]);

        assert_eq!(
            comparison.missing_percentage(FeerateDiagramDiffUnit::Weight),
            Some(1.0)
        );
        assert_eq!(
            comparison.extra_percentage(FeerateDiagramDiffUnit::Weight),
            Some(0.0)
        );
    }

    #[test]
    fn an_empty_node_diagram_gives_no_reading() {
        let comparison = compare(&[], &node_diagram());

        for unit in [
            FeerateDiagramDiffUnit::Weight,
            FeerateDiagramDiffUnit::ChunkCount,
        ] {
            assert_eq!(comparison.missing_percentage(unit), None);
            assert_eq!(comparison.extra_percentage(unit), None);
        }

        let comparison = compare(&[], &[]);

        assert_eq!(
            comparison.missing_percentage(FeerateDiagramDiffUnit::Weight),
            None
        );
    }
}
