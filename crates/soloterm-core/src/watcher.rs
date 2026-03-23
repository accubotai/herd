use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// File watcher that triggers process restarts on changes
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

/// A debounced file change event
#[derive(Debug, Clone)]
pub struct FileChange {
    pub paths: Vec<PathBuf>,
    pub process_name: String,
}

impl FileWatcher {
    /// Create a new file watcher for a process.
    /// Watches the given paths and sends change events through the channel.
    pub fn new(
        process_name: String,
        watch_paths: Vec<PathBuf>,
        ignore_paths: Vec<PathBuf>,
        tx: mpsc::UnboundedSender<FileChange>,
    ) -> notify::Result<Self> {
        let name = process_name.clone();
        let mut last_event = Instant::now();
        let debounce = Duration::from_millis(500);

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Debounce: skip events within 500ms of the last one
                let now = Instant::now();
                if now.duration_since(last_event) < debounce {
                    return;
                }
                last_event = now;

                // Filter out ignored paths
                let relevant_paths: Vec<PathBuf> = event
                    .paths
                    .into_iter()
                    .filter(|p| {
                        !ignore_paths.iter().any(|ignored| p.starts_with(ignored))
                    })
                    .collect();

                if !relevant_paths.is_empty() {
                    let _ = tx.send(FileChange {
                        paths: relevant_paths,
                        process_name: name.clone(),
                    });
                }
            }
        })?;

        for path in watch_paths {
            watcher.watch(&path, RecursiveMode::Recursive)?;
        }

        Ok(Self { _watcher: watcher })
    }
}
