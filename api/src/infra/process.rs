//! CPU and memory of the api process, read from `/proc`.
//!
//! Linux-only, which the containers make safe and which keeps this
//! dependency-free. Elsewhere the sampler warns once and emits nothing:
//! absent series, rather than zeroes that would read as an idle process.

use std::sync::{Once, OnceLock};

/// Cumulative CPU time, user + system.
///
/// A monotonic float gauge rather than a counter: `metrics::Counter` is
/// `u64`-only, and whole seconds would read as a flat zero. `rate()` handles
/// it identically, restart drop included.
const CPU_SECONDS_TOTAL: &str = "process_cpu_seconds_total";

const CPU_USER_SECONDS_TOTAL: &str = "process_cpu_user_seconds_total";
const CPU_SYSTEM_SECONDS_TOTAL: &str = "process_cpu_system_seconds_total";
const RESIDENT_MEMORY_BYTES: &str = "process_resident_memory_bytes";

/// High-water mark since start, so a spike between two samples survives.
const RESIDENT_MEMORY_PEAK_BYTES: &str = "process_resident_memory_peak_bytes";

const VIRTUAL_MEMORY_BYTES: &str = "process_virtual_memory_bytes";
const THREADS: &str = "process_threads";

/// Constant per process, so a change in it is a restart.
const START_TIME_SECONDS: &str = "process_start_time_seconds";

/// Ticks per second that `/proc` counts CPU in. Fixed at 100 on Linux; reading
/// it properly would mean linking libc for `sysconf(_SC_CLK_TCK)`.
const USER_HZ: f64 = 100.0;

const STAT_PATH: &str = "/proc/self/stat";
const STATUS_PATH: &str = "/proc/self/status";

/// Read only for `btime`.
const BOOT_TIME_PATH: &str = "/proc/stat";

/// Sets every process gauge from one reading of `/proc`.
pub fn sample() {
    let Some((stat, status)) = read() else {
        warn_once();
        return;
    };

    metrics::gauge!(CPU_USER_SECONDS_TOTAL).set(stat.cpu_user_seconds);
    metrics::gauge!(CPU_SYSTEM_SECONDS_TOTAL).set(stat.cpu_system_seconds);
    metrics::gauge!(CPU_SECONDS_TOTAL).set(stat.cpu_user_seconds + stat.cpu_system_seconds);
    metrics::gauge!(THREADS).set(stat.threads);

    metrics::gauge!(RESIDENT_MEMORY_BYTES).set(status.resident_bytes);
    metrics::gauge!(RESIDENT_MEMORY_PEAK_BYTES).set(status.resident_peak_bytes);
    metrics::gauge!(VIRTUAL_MEMORY_BYTES).set(status.virtual_bytes);

    if let Some(start_time) = start_time_seconds(stat.start_time_ticks) {
        metrics::gauge!(START_TIME_SECONDS).set(start_time);
    }
}

/// Fields of [`STAT_PATH`], converted to seconds.
struct Stat {
    cpu_user_seconds: f64,
    cpu_system_seconds: f64,
    threads: f64,
    start_time_ticks: f64,
}

/// Fields of [`STATUS_PATH`], converted to bytes.
struct Status {
    resident_bytes: f64,
    resident_peak_bytes: f64,
    virtual_bytes: f64,
}

fn read() -> Option<(Stat, Status)> {
    let stat = parse_stat(&std::fs::read_to_string(STAT_PATH).ok()?)?;
    let status = parse_status(&std::fs::read_to_string(STATUS_PATH).ok()?)?;
    Some((stat, status))
}

/// The second field is the executable name in parens and may itself contain
/// spaces and parens, so fields are split after its *last* closing paren.
/// Indices are into that remainder, where 0 is `state` -- field 3 of
/// proc_pid_stat(5).
fn parse_stat(raw: &str) -> Option<Stat> {
    let rest = &raw[raw.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_ascii_whitespace().collect();

    Some(Stat {
        cpu_user_seconds: ticks_to_seconds(fields.get(11)?)?,
        cpu_system_seconds: ticks_to_seconds(fields.get(12)?)?,
        threads: fields.get(17)?.parse().ok()?,
        start_time_ticks: fields.get(19)?.parse().ok()?,
    })
}

fn ticks_to_seconds(field: &str) -> Option<f64> {
    Some(field.parse::<f64>().ok()? / USER_HZ)
}

fn parse_status(raw: &str) -> Option<Status> {
    Some(Status {
        resident_bytes: kilobytes_field(raw, "VmRSS:")?,
        resident_peak_bytes: kilobytes_field(raw, "VmHWM:")?,
        virtual_bytes: kilobytes_field(raw, "VmSize:")?,
    })
}

/// Reads one `<key> <n> kB` line, returning bytes.
fn kilobytes_field(raw: &str, key: &str) -> Option<f64> {
    let line = raw.lines().find(|line| line.starts_with(key))?;
    let kilobytes: f64 = line.split_ascii_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes * 1024.0)
}

/// [`STAT_PATH`] measures the start from boot, so this adds `btime`. Cached
/// because `btime` is derived from the current clock rather than stored, and
/// shifts whenever that clock is corrected -- recomputing each tick would drift
/// the start time and read as a restart.
fn start_time_seconds(start_time_ticks: f64) -> Option<f64> {
    static START_TIME: OnceLock<Option<f64>> = OnceLock::new();
    *START_TIME.get_or_init(|| {
        let boot_time = parse_boot_time(&std::fs::read_to_string(BOOT_TIME_PATH).ok()?)?;
        Some(boot_time + start_time_ticks / USER_HZ)
    })
}

fn parse_boot_time(raw: &str) -> Option<f64> {
    raw.lines()
        .find(|line| line.starts_with("btime "))?
        .split_ascii_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// Once, not once per tick: a `/proc` that cannot be read now will not start
/// working later in the same process.
fn warn_once() {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| {
        tracing::warn!("cannot read /proc: process cpu and memory metrics are unavailable");
    });
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    /// 250 and 125 ticks are 2.5s and 1.25s, which no swapped field produces.
    const STAT: &str = "4242 (api) S 1 4242 4242 0 -1 4194560 12345 0 3 0 250 125 0 0 20 0 17 0 \
                        2892562 3358720 418";

    const STATUS: &str = "Name:\tapi\nThreads:\t17\nVmSize:\t 1263796 kB\nVmHWM:\t    9000 kB\n\
                          VmRSS:\t    7648 kB\n";

    #[test]
    fn stat_converts_ticks_to_seconds() {
        let stat = parse_stat(STAT).unwrap();
        assert_eq!(stat.cpu_user_seconds, 2.5);
        assert_eq!(stat.cpu_system_seconds, 1.25);
    }

    #[test]
    fn stat_reads_threads_and_start_time() {
        let stat = parse_stat(STAT).unwrap();
        assert_eq!(stat.threads, 17.0);
        assert_eq!(stat.start_time_ticks, 2_892_562.0);
    }

    /// Why the split is on the last paren: this shifts every later field
    /// otherwise.
    #[test]
    fn stat_survives_a_process_name_with_spaces_and_parens() {
        let raw = STAT.replace("(api)", "(we ird (name))");
        let stat = parse_stat(&raw).unwrap();
        assert_eq!(stat.cpu_user_seconds, 2.5);
        assert_eq!(stat.threads, 17.0);
    }

    #[test]
    fn stat_rejects_a_truncated_line() {
        assert!(parse_stat("4242 (api) S 1 4242").is_none());
        assert!(parse_stat("nonsense without a paren").is_none());
    }

    /// Confusing the two would understate memory 1024x and still look sane.
    #[test]
    fn status_converts_kilobytes_to_bytes() {
        let status = parse_status(STATUS).unwrap();
        assert_eq!(status.resident_bytes, 7648.0 * 1024.0);
        assert_eq!(status.resident_peak_bytes, 9000.0 * 1024.0);
        assert_eq!(status.virtual_bytes, 1_263_796.0 * 1024.0);
    }

    #[test]
    fn status_rejects_a_missing_field() {
        assert!(parse_status("Name:\tapi\nVmSize:\t 100 kB\n").is_none());
    }

    #[test]
    fn boot_time_is_read_from_its_line() {
        let raw = "cpu  1 2 3\nbtime 1751587200\nprocesses 99\n";
        assert_eq!(parse_boot_time(raw), Some(1_751_587_200.0));
    }

    #[test]
    fn boot_time_rejects_a_file_without_btime() {
        assert!(parse_boot_time("cpu  1 2 3\nprocesses 99\n").is_none());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod sample_tests {
    use super::*;
    use testkit::metrics::capture;

    fn value_of(rendered: &str, name: &str) -> f64 {
        let line = rendered
            .lines()
            .find(|line| line.starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("expected series `{name}` in:\n{rendered}"));
        line.split_ascii_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn sample_emits_every_series_for_the_live_process() {
        let rendered = capture(async { sample() });
        for name in [
            CPU_SECONDS_TOTAL,
            CPU_USER_SECONDS_TOTAL,
            CPU_SYSTEM_SECONDS_TOTAL,
            RESIDENT_MEMORY_BYTES,
            RESIDENT_MEMORY_PEAK_BYTES,
            VIRTUAL_MEMORY_BYTES,
            THREADS,
            START_TIME_SECONDS,
        ] {
            value_of(&rendered, name);
        }
    }

    /// The unit conversion checked against a real process: under a megabyte, or
    /// more than the machine has, means kB and bytes were confused.
    #[test]
    fn resident_memory_is_a_plausible_byte_count() {
        let rendered = capture(async { sample() });
        let resident = value_of(&rendered, RESIDENT_MEMORY_BYTES);
        assert!(
            (1_000_000.0..1e12).contains(&resident),
            "implausible resident memory: {resident} bytes"
        );
        assert!(value_of(&rendered, RESIDENT_MEMORY_PEAK_BYTES) >= resident);
    }

    /// The boot-time arithmetic has no other check: an offset applied the wrong
    /// way lands decades out while still being a number.
    #[test]
    fn start_time_is_in_the_past_but_within_this_boot() {
        let rendered = capture(async { sample() });
        let start_time = value_of(&rendered, START_TIME_SECONDS);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(
            start_time <= now,
            "start time {start_time} is in the future"
        );
        assert!(
            now - start_time < 365.0 * 24.0 * 3600.0,
            "start time {start_time} predates boot"
        );
    }

    #[test]
    fn cpu_total_is_its_user_and_system_parts() {
        let rendered = capture(async { sample() });
        let total = value_of(&rendered, CPU_SECONDS_TOTAL);
        let user = value_of(&rendered, CPU_USER_SECONDS_TOTAL);
        let system = value_of(&rendered, CPU_SYSTEM_SECONDS_TOTAL);
        assert!((total - (user + system)).abs() < 1e-9);
    }
}
