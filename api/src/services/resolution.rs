//! The bucket-size ladder shared by every time-series read path (gauges,
//! counters, ...). Both charts must agree on what bucket a given time range
//! maps to, or their x-axes desync.
use crate::db::NATIVE_RESOLUTION_SECS;
use time::Duration;

/// Bucket sizes the read path may choose, smallest (native) first.
const RESOLUTION_LADDER_SECS: [i64; 7] = [
    NATIVE_RESOLUTION_SECS, // 1 min
    5 * 60,
    10 * 60,
    30 * 60,
    60 * 60,      // 1 hour
    6 * 60 * 60,  // 6 hours
    24 * 60 * 60, // 1 day
];

/// Points a series may hold; caps how many buckets a range gets divided into.
pub const MAX_POINTS: i64 = 2000;

/// Default lookback for a range with no explicit `from`.
pub const DEFAULT_RANGE: Duration = Duration::hours(24);

/// Smallest ladder rung that keeps `span` within `MAX_POINTS` buckets, or the
/// widest rung if even that does not fit.
pub fn pick_resolution(span: Duration) -> i64 {
    let span_secs = span.whole_seconds();
    RESOLUTION_LADDER_SECS
        .into_iter()
        .find(|res| span_secs / res <= MAX_POINTS)
        .unwrap_or(*RESOLUTION_LADDER_SECS.last().expect("ladder is not empty"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_resolution_uses_native_cadence_for_a_day() {
        assert_eq!(pick_resolution(Duration::hours(24)), 60);
    }

    #[test]
    fn pick_resolution_steps_up_the_ladder_as_the_range_widens() {
        assert_eq!(pick_resolution(Duration::days(3)), 300);
        assert_eq!(pick_resolution(Duration::days(30)), 1_800);
        assert_eq!(pick_resolution(Duration::days(200)), 21_600);
    }

    #[test]
    fn pick_resolution_keeps_points_within_the_cap_when_a_rung_fits() {
        let span = Duration::days(200);
        let points = span.whole_seconds() / pick_resolution(span);
        assert!(points <= MAX_POINTS);
    }

    #[test]
    fn pick_resolution_clamps_to_the_widest_rung_for_an_extreme_range() {
        // 10 years: even the widest (daily) bucket exceeds MAX_POINTS, so the
        // ladder just stops climbing instead of inventing a coarser one.
        assert_eq!(pick_resolution(Duration::days(3_650)), 86_400);
    }
}
