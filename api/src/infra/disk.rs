use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Instant;

const DIRECTORY_BYTES: &str = "data_directory_bytes";
const SCAN_SECONDS: &str = "data_directory_scan_seconds";

/// Entries the walk could not read. Without it an unreadable subtree is
/// indistinguishable from an empty one, and the gauges quietly understate.
const SCAN_ERRORS_TOTAL: &str = "data_directory_scan_errors_total";

/// `st_blocks` counts 512-byte units whatever the filesystem's own block size,
/// so this factor is fixed rather than queried.
const BLOCK_SIZE: u64 = 512;

/// Sets the size gauges from one walk of every subdirectory of `root`.
pub fn sample(root: &Path) {
    let start = Instant::now();

    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) => return warn_once(root, &e),
    };

    for entry in entries {
        let Ok(entry) = entry else {
            count_error();
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            count_error();
            continue;
        };
        // Only immediate subdirectories become series, which keeps the `dir`
        // label bounded by what the init-dirs service creates. Loose files at
        // the root belong to no service and are left out.
        if !metadata.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(bytes) = walk(&entry.path()) else {
            continue;
        };

        metrics::gauge!(DIRECTORY_BYTES, "dir" => name).set(bytes as f64);
    }

    metrics::histogram!(SCAN_SECONDS).record(start.elapsed().as_secs_f64());
}

/// Sums block usage under `root`, the directories' own inodes included.
///
/// Symlinks are neither followed nor counted.
fn walk(root: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;

    // A root that cannot be opened yields no series at all.
    if std::fs::read_dir(root).is_err() {
        count_error();
        return None;
    }

    let mut bytes = 0;
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        // Each directory is pushed once, so its own inode is charged once.
        match std::fs::symlink_metadata(&dir) {
            Ok(metadata) => bytes += metadata.blocks() * BLOCK_SIZE,
            Err(_) => count_error(),
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                count_error();
                continue;
            }
        };

        for entry in entries {
            let Ok(entry) = entry else {
                count_error();
                continue;
            };
            // DirEntry::metadata does not traverse symlinks, so a link is
            // described here rather than its target.
            let Ok(metadata) = entry.metadata() else {
                count_error();
                continue;
            };

            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }

            bytes += metadata.blocks() * BLOCK_SIZE;
        }
    }

    Some(bytes)
}

fn count_error() {
    metrics::counter!(SCAN_ERRORS_TOTAL).increment(1);
}

/// Once per process: a data root that is missing is a deployment fact, and
/// re-stating it every minute would bury the rest of the log.
fn warn_once(root: &Path, error: &std::io::Error) {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| {
        tracing::warn!(
            "cannot read {}: data directory metrics are unavailable: {error}",
            root.display()
        );
    });
}

/// Resolves `DATA_DIR` for the sampler, absolute so a later working-directory
/// change cannot repoint it.
pub fn root(configured: &str) -> PathBuf {
    let path = Path::new(configured);
    match path.is_absolute() {
        true => path.to_path_buf(),
        false => std::env::current_dir().unwrap_or_default().join(path),
    }
}

#[cfg(test)]
mod tree {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A throwaway directory tree under the system temp dir.
    pub struct TempTree(pub PathBuf);

    impl TempTree {
        pub fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "data-directory-metrics-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&path).expect("failed to create temp tree");
            TempTree(path)
        }

        pub fn dir(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(&path).expect("failed to create directory");
            path
        }

        pub fn file(&self, rel: &str, bytes: usize) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().expect("file has a parent"))
                .expect("failed to create parent");
            std::fs::write(&path, vec![0u8; bytes]).expect("failed to write file");
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod walk_tests {
    use super::tree::TempTree;
    use super::*;

    const MIB: usize = 1024 * 1024;

    fn walked(root: &Path) -> u64 {
        walk(root).expect("root should be readable")
    }

    #[test]
    fn walk_reports_nothing_when_the_root_cannot_be_read() {
        let tree = TempTree::new();
        tree.file("not-a-directory", 1);

        assert!(walk(&tree.0.join("not-a-directory")).is_none());
    }

    #[test]
    fn walk_reaches_files_at_every_depth() {
        let tree = TempTree::new();
        tree.file("a.dat", MIB);
        tree.file("nested/b.dat", MIB);
        tree.file("nested/deeper/c.dat", MIB);

        assert!(walked(&tree.0) >= 3 * MIB as u64);
    }

    /// The distinction the module exists for: sixteen one-byte files occupy
    /// far more than sixteen bytes once rounded up to blocks.
    #[test]
    fn walk_reports_block_usage_rather_than_apparent_size() {
        let tree = TempTree::new();
        for i in 0..16 {
            tree.file(&format!("tiny-{i}.dat"), 1);
        }

        let bytes = walked(&tree.0);
        assert!(
            bytes > 16,
            "block usage {bytes} did not exceed the 16 bytes of apparent size"
        );
    }

    /// Catches a dropped [`BLOCK_SIZE`] factor, which would understate 512x
    /// and still read as a plausible number.
    #[test]
    fn walk_charges_a_large_file_its_full_size() {
        let tree = TempTree::new();
        tree.file("big.dat", MIB);

        assert!(walked(&tree.0) >= MIB as u64);
    }

    /// A link out of the tree would otherwise charge a directory for bytes it
    /// does not own.
    #[test]
    fn walk_neither_follows_nor_counts_symlinks() {
        let tree = TempTree::new();
        tree.file("real/big.dat", MIB);
        let linked = tree.dir("linked");
        std::os::unix::fs::symlink(tree.0.join("real"), linked.join("shortcut"))
            .expect("failed to create symlink");

        assert!(walked(&linked) < MIB as u64);
    }

    #[test]
    fn root_resolves_a_relative_path_to_an_absolute_one() {
        assert!(root("data").is_absolute());
        assert_eq!(root("/srv/data"), Path::new("/srv/data"));
    }
}

#[cfg(test)]
mod sample_tests {
    use super::tree::TempTree;
    use super::*;
    use testkit::metrics::{assert_no_series, assert_series, capture};

    const MIB: usize = 1024 * 1024;

    fn series_of(rendered: &str, dir: &str) -> f64 {
        let prefix = format!(r#"{DIRECTORY_BYTES}{{dir="{dir}"}} "#);
        let line = rendered
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("expected a series for `{dir}` in:\n{rendered}"));
        line.rsplit(' ')
            .next()
            .expect("series has a value")
            .parse()
            .expect("value is a number")
    }

    #[test]
    fn sample_emits_one_series_per_child_directory() {
        let tree = TempTree::new();
        tree.file("alpha/one.dat", MIB);
        tree.file("beta/one.dat", MIB);

        let rendered = capture(async { sample(&tree.0) });

        assert!(series_of(&rendered, "alpha") >= MIB as f64);
        assert!(series_of(&rendered, "beta") >= MIB as f64);
    }

    /// A file sitting loose at the root belongs to no service, so it must not
    /// become a series of its own.
    #[test]
    fn sample_ignores_loose_files_at_the_root() {
        let tree = TempTree::new();
        tree.file("alpha/one.dat", MIB);
        tree.file("loose.dat", MIB);

        let rendered = capture(async { sample(&tree.0) });

        assert!(series_of(&rendered, "alpha") >= MIB as f64);
        assert_no_series(&rendered, r#"data_directory_bytes{dir="loose.dat"}"#);
    }

    /// The `data/postgres` case: a directory the process cannot open must not
    /// become a near-empty series instead of no series.
    #[test]
    fn sample_skips_a_child_directory_it_cannot_read() {
        use std::os::unix::fs::PermissionsExt;

        let tree = TempTree::new();
        tree.file("open/one.dat", MIB);
        tree.file("closed/hidden.dat", MIB);
        let closed = tree.dir("closed");
        std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o000))
            .expect("failed to close directory");

        // Root ignores the mode bits, so there is nothing to assert there.
        let readable_anyway = std::fs::read_dir(&closed).is_ok();
        let rendered = capture(async { sample(&tree.0) });
        std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o755))
            .expect("failed to reopen directory");
        if readable_anyway {
            return;
        }

        assert!(series_of(&rendered, "open") >= MIB as f64);
        assert_no_series(&rendered, r#"data_directory_bytes{dir="closed"}"#);
    }

    #[test]
    fn sample_records_what_the_scan_cost() {
        let tree = TempTree::new();
        tree.file("alpha/one.dat", 1);

        let rendered = capture(async { sample(&tree.0) });

        assert_series(&rendered, "data_directory_scan_seconds_count 1");
    }

    /// Absent series, never zeroes: a zero would read as an empty directory
    /// rather than as a data root that is not mounted.
    #[test]
    fn sample_emits_nothing_when_the_root_is_missing() {
        let missing = std::env::temp_dir().join("data-directory-metrics-absent");

        let rendered = capture(async { sample(&missing) });

        assert_no_series(&rendered, "data_directory_bytes");
    }
}
