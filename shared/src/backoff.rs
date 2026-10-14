use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

/// Scales `delay` by a factor in [1 - spread, 1 + spread] picked by hashing `key`.
///
/// Jitter only needs distinct keys to land apart, not unpredictability, so the
/// factor comes from a fixed-key hash instead of an RNG: the same key always
/// gets the same delay, which keeps callers' timing reproducible in tests.
/// Include whatever varies between tries (e.g. the attempt count) in `key`.
pub fn jittered<K: Hash + ?Sized>(delay: Duration, spread: f64, key: &K) -> Duration {
    delay.mul_f64(1.0 - spread + 2.0 * spread * unit_from(key))
}

/// Uniform in [0, 1): the top 53 bits of the hash fill an f64 mantissa exactly.
fn unit_from<K: Hash + ?Sized>(key: &K) -> f64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_secs(60);

    #[test]
    fn jittered_stays_within_the_spread() {
        for key in 0..10_000u32 {
            let d = jittered(DELAY, 0.2, &key);
            assert!(
                (Duration::from_secs(48)..Duration::from_secs(72)).contains(&d),
                "{d:?} for key {key}"
            );
        }
    }

    #[test]
    fn jittered_is_stable_for_a_key() {
        assert_eq!(
            jittered(DELAY, 0.2, &("txid", 3u8)),
            jittered(DELAY, 0.2, &("txid", 3u8))
        );
    }

    #[test]
    fn jittered_spreads_distinct_keys_across_the_range() {
        let delays: Vec<Duration> = (0..1_000u32).map(|k| jittered(DELAY, 0.2, &k)).collect();
        let min = delays.iter().min().unwrap();
        let max = delays.iter().max().unwrap();
        assert!(*min < Duration::from_secs(50), "min {min:?}");
        assert!(*max > Duration::from_secs(70), "max {max:?}");
    }
}
