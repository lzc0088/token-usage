//! File watcher + trailing debounce (T1.4).
//!
//! Watches the tool data directories discovered in T1.3 (`paths::watch_paths`)
//! via the `notify` crate, coalesces bursts of writes into a single "tick" after
//! a 1.5 s quiet period (so a tool rapidly appending logs triggers one re-scan,
//! not hundreds), and forwards ticks on a tokio channel for the scheduler (T1.5).
//!
//! Self-trigger loop protection (design §4.2): tokscale writes to its own cache
//! dirs while scanning; watching those would retrigger endlessly. We filter them
//! out of the watch list before starting `notify`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

/// Default quiet period before a coalesced tick fires (design §4.2: 1.5 s).
pub const DEFAULT_DEBOUNCE_MS: u64 = 1500;

/// True for tokscale's own cache/output dirs — watching them risks a self-trigger
/// loop (scan writes → event → re-scan). Component-based for cross-platform safety.
pub fn is_self_trigger(path: &Path) -> bool {
    let comps: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    let has_tokscale = comps.contains(&"tokscale");
    if !has_tokscale {
        return false;
    }
    // tokscale-managed caches it writes during scanning. `headless` session dirs
    // are real client data, so they stay watched.
    const CACHE: &[&str] = &["cursor-cache", "antigravity-cache"];
    comps.iter().any(|c| CACHE.contains(c))
}

/// Dedup + drop self-trigger paths. Pure (no fs access).
pub fn filter_watch_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|p| !is_self_trigger(p))
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

/// Guard returned by [`spawn`]; dropping it stops watching.
pub struct WatchGuard {
    debounce: tokio::task::JoinHandle<()>,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.debounce.abort();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("no watchable dirs (all filtered out)")]
    NoDirs,
}

/// Start watching `dirs` (recursive). After `debounce_ms` of quiet following a
/// burst of changes, a single `()` tick is sent on `tick_tx`. Returns a guard
/// that stops the watcher when dropped.
///
/// Internally: notify → std channel → forwarder thread → tokio debounce task.
pub fn spawn(
    dirs: Vec<PathBuf>,
    debounce_ms: u64,
    tick_tx: mpsc::Sender<()>,
) -> Result<WatchGuard, WatcherError> {
    if dirs.is_empty() {
        return Err(WatcherError::NoDirs);
    }

    // notify → std channel (notify invokes its callback on an internal thread).
    let (raw_tx, raw_rx) = std::sync::mpsc::channel();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            // Errors here are non-fatal (single event lost); debounce survives.
            if res.is_ok() {
                let _ = raw_tx.send(());
            }
        })?;

    for d in &dirs {
        watcher.watch(d, RecursiveMode::Recursive)?;
    }

    // Bridge std channel → tokio via a dedicated OS thread that also owns the
    // watcher (so it lives exactly as long as we want to watch).
    let (forward_tx, mut forward_rx) = mpsc::channel::<()>(64);
    std::thread::spawn(move || {
        let _watcher = watcher; // keep alive for the thread's lifetime
        while raw_rx.recv().is_ok() {
            if forward_tx.blocking_send(()).is_err() {
                break; // debounce task gone → stop
            }
        }
    });

    let quiet = Duration::from_millis(debounce_ms.max(50));
    let debounce = tokio::spawn(async move {
        loop {
            // Wait for the first event of a burst.
            if forward_rx.recv().await.is_none() {
                return;
            }
            // Reset the quiet timer on each subsequent event; emit a tick once
            // `quiet` elapses with no new events.
            loop {
                tokio::select! {
                    Some(()) = forward_rx.recv() => continue,
                    _ = tokio::time::sleep(quiet) => {
                        let _ = tick_tx.send(()).await;
                        break;
                    }
                }
            }
        }
    });

    Ok(WatchGuard { debounce })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_self_trigger_caches() {
        assert!(is_self_trigger(Path::new(
            "/home/u/.config/tokscale/cursor-cache"
        )));
        assert!(is_self_trigger(Path::new(
            "/home/u/.config/tokscale/antigravity-cache/sessions"
        )));
    }

    #[test]
    fn keeps_real_client_dirs_and_headless() {
        // real tool dirs (no tokscale component) stay
        assert!(!is_self_trigger(Path::new("/home/u/.claude/projects")));
        assert!(!is_self_trigger(Path::new("/home/u/.codex/sessions")));
        // tokscale headless session data is real → keep
        assert!(!is_self_trigger(Path::new(
            "/home/u/.config/tokscale/headless/codex"
        )));
    }

    #[test]
    fn filter_dedupes_and_excludes_self_trigger() {
        let paths = vec![
            PathBuf::from("/a/claude/projects"),
            PathBuf::from("/a/claude/projects"), // dup
            PathBuf::from("/a/.config/tokscale/cursor-cache"), // excluded
            PathBuf::from("/a/codex/sessions"),
        ];
        let out = filter_watch_paths(paths);
        assert_eq!(
            out,
            vec![
                PathBuf::from("/a/claude/projects"),
                PathBuf::from("/a/codex/sessions"),
            ]
        );
    }

    #[test]
    fn spawn_errors_on_empty_dirs() {
        let (tx, _rx) = mpsc::channel::<()>(1);
        assert!(matches!(spawn(vec![], 100, tx), Err(WatcherError::NoDirs)));
    }

    #[tokio::test]
    async fn emits_one_tick_for_a_burst_of_writes() {
        // Integration: watch a temp dir, write several files in quick succession,
        // expect exactly one coalesced tick within the debounce window + margin.
        let dir = std::env::temp_dir().join("tu_test_watcher_burst");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (tx, mut rx) = mpsc::channel::<()>(8);
        let _guard = spawn(vec![dir.clone()], 200, tx).unwrap();

        // Give notify a moment to register the watch.
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Burst: 5 writes in < debounce window.
        for i in 0..5 {
            std::fs::write(dir.join(format!("f{i}.txt")), b"x").unwrap();
        }

        // First tick lands after ~200ms quiet.
        let t1 = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(t1.unwrap().is_some(), "expected a tick after the burst");

        // And no second tick shortly after (debounce coalesced the burst).
        let t2 = tokio::time::timeout(Duration::from_millis(400), rx.recv()).await;
        assert!(
            t2.is_err() || t2.unwrap().is_none(),
            "expected no extra tick"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
