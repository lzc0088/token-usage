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

use notify::{PollWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

/// Default quiet period before a coalesced tick fires (design §4.2: 1.5 s).
pub const DEFAULT_DEBOUNCE_MS: u64 = 1500;

/// Polling interval used when native watch descriptors are exhausted and the
/// watcher is rebuilt as a poller. Polling needs no inotify/FSEvents descriptors.
const POLL_FALLBACK_INTERVAL_SECS: u64 = 2;

/// OS errnos signaling that no more file-watch descriptors are available and
/// native watching is dead for this process. Linux inotify surfaces ENOSPC (28)
/// when the per-user watch limit is hit; EMFILE (24) / ENFILE (23) are the
/// per-process / system-wide fd-table limits. notify also surfaces the inotify
/// case as a dedicated `MaxFilesWatch` kind (see [`is_descriptor_exhaustion`]).
const DESCRIPTOR_EXHAUSTION_ERRNOS: &[i32] = &[28, 24, 23];

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

/// Signal from the notify callback to the forwarder thread.
#[derive(Clone, Copy)]
enum WatchSignal {
    /// A filesystem event was observed.
    Event,
    /// Watch descriptors are exhausted — the native watcher can no longer
    /// deliver events and must be rebuilt as a poller (sticky, once).
    DescriptorExhausted,
}

/// True if a notify error means watch descriptors are exhausted (inotify
/// per-user limit, or process/system fd-table limit). When notify reports any
/// of these, native watching is dead for this process: without intervention the
/// watcher silently stops receiving events and live mode degrades to the
/// history timer with no indication. The caller must rebuild as a poller.
fn is_descriptor_exhaustion(e: &notify::Error) -> bool {
    match e.kind {
        // notify surfaces the inotify watch-limit as a dedicated kind.
        notify::ErrorKind::MaxFilesWatch => true,
        notify::ErrorKind::Io(ref io) => matches!(
            io.raw_os_error(),
            Some(code) if DESCRIPTOR_EXHAUSTION_ERRNOS.contains(&code)
        ),
        _ => false,
    }
}

/// Build the notify event handler that forwards events (and descriptor
/// exhaustion) onto `tx`. Other errors are dropped — a single lost event is
/// non-fatal since the debounce layer coalesces bursts.
fn make_handler(
    tx: std::sync::mpsc::Sender<WatchSignal>,
) -> impl FnMut(notify::Result<notify::Event>) + Send + 'static {
    move |res: notify::Result<notify::Event>| match res {
        Ok(_) => {
            let _ = tx.send(WatchSignal::Event);
        }
        Err(e) => {
            if is_descriptor_exhaustion(&e) {
                let _ = tx.send(WatchSignal::DescriptorExhausted);
            }
        }
    }
}

/// Start watching `dirs` (recursive). After `debounce_ms` of quiet following a
/// burst of changes, a single `()` tick is sent on `tick_tx`. Returns a guard
/// that stops the watcher when dropped.
///
/// Internally: notify → std channel → forwarder thread → tokio debounce task.
///
/// If the OS runs out of watch descriptors (common on Linux with many tools —
/// inotify has a shared per-user budget that editors also consume), notify
/// reports the error asynchronously. The forwarder thread catches it and
/// rebuilds the watcher as a 2 s poller (sticky for the process lifetime),
/// keeping live updates working instead of silently degrading to the history
/// timer.
pub fn spawn(
    dirs: Vec<PathBuf>,
    debounce_ms: u64,
    tick_tx: mpsc::Sender<()>,
) -> Result<WatchGuard, WatcherError> {
    if dirs.is_empty() {
        return Err(WatcherError::NoDirs);
    }

    // notify → std channel (notify invokes its callback on an internal thread).
    let (raw_tx, raw_rx) = std::sync::mpsc::channel::<WatchSignal>();

    // Initial native watcher.
    let mut native = notify::recommended_watcher(make_handler(raw_tx.clone()))?;
    for d in &dirs {
        native.watch(d, RecursiveMode::Recursive)?;
    }

    // Bridge std channel → tokio via a dedicated OS thread that also owns the
    // watcher (so it lives exactly as long as we want to watch). On descriptor
    // exhaustion it rebuilds (sticky) as a poll watcher watching the same dirs.
    let dirs_for_thread = dirs.clone();
    let (forward_tx, mut forward_rx) = mpsc::channel::<()>(64);
    let poll_interval = Duration::from_secs(POLL_FALLBACK_INTERVAL_SECS);
    std::thread::spawn(move || {
        // Held as `Box<dyn Watcher>` purely for liveness: we `watch()` the dirs
        // before boxing, then keep the boxed watcher alive until rebuild/drop.
        // `RecommendedWatcher` and `PollWatcher` are distinct concrete types, so
        // type-erase via the `Watcher` trait to hold either in one slot.
        let mut current: Option<Box<dyn Watcher>> = Some(Box::new(native));
        let mut polling = false; // sticky for the process lifetime
        while let Ok(sig) = raw_rx.recv() {
            match sig {
                WatchSignal::Event => {
                    if forward_tx.blocking_send(()).is_err() {
                        break; // debounce task gone → stop
                    }
                }
                WatchSignal::DescriptorExhausted if !polling => {
                    polling = true;
                    tracing::warn!(
                        "file watcher descriptors exhausted (ENOSPC/EMFILE/ENFILE); \
                         switching to a {}s polling fallback for this session",
                        POLL_FALLBACK_INTERVAL_SECS
                    );
                    // Drop the dead native watcher; it cannot recover.
                    current.take();
                    match PollWatcher::new(
                        make_handler(raw_tx.clone()),
                        notify::Config::default().with_poll_interval(poll_interval),
                    ) {
                        Ok(mut w) => {
                            for d in &dirs_for_thread {
                                let _ = w.watch(d, RecursiveMode::Recursive);
                            }
                            current = Some(Box::new(w));
                        }
                        Err(e) => {
                            tracing::error!(
                                "polling watcher rebuild failed: {e}; live updates \
                                 disabled, relying on the history timer only"
                            );
                        }
                    }
                }
                // Sticky: ignore further exhaustion signals once polling.
                _ => {}
            }
        }
        // `current` (and its notify handler) dropped here on channel close.
        drop(current);
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

    #[test]
    fn detects_descriptor_exhaustion_errors() {
        // inotify watch limit → dedicated kind.
        let max_files = notify::Error::new(notify::ErrorKind::MaxFilesWatch);
        assert!(is_descriptor_exhaustion(&max_files));
        // ENOSPC / EMFILE / ENFILE surfaced as Io errors → all caught.
        for code in [28i32, 24, 23] {
            let e = notify::Error::io(std::io::Error::from_raw_os_error(code));
            assert!(
                is_descriptor_exhaustion(&e),
                "errno {code} should be treated as descriptor exhaustion"
            );
        }
        // Unrelated I/O error (EIO=5) and generic errors → not exhaustion.
        let eio = notify::Error::io(std::io::Error::from_raw_os_error(5));
        assert!(!is_descriptor_exhaustion(&eio));
        let generic = notify::Error::generic("boom");
        assert!(!is_descriptor_exhaustion(&generic));
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

        // Burst: 5 writes. Small delays between writes ensure the OS delivers
        // each event individually (prevents OS-level batching that can split the
        // burst across multiple debounce cycles on constrained CI runners).
        for i in 0..5 {
            std::fs::write(dir.join(format!("f{i}.txt")), b"x").unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // First tick lands after ~200ms quiet.
        let t1 = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(t1.unwrap().is_some(), "expected a tick after the burst");

        // Wait a generous period to confirm no second tick fires (debounce
        // coalesced the entire burst into one tick).
        let t2 = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await;
        assert!(
            t2.is_err() || t2.unwrap().is_none(),
            "expected no extra tick"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
