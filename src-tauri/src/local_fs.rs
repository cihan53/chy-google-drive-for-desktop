use tauri::{AppHandle, Emitter, Manager};
use crate::config::AppConfig;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use rayon::prelude::*;


#[derive(Clone, serde::Serialize)]
pub struct ScanProgress {
    #[serde(rename = "fileCount")]
    pub file_count: u64,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    #[serde(rename = "isDone")]
    pub is_done: bool,
}

/// Pre-compiled regex patterns for the duration of a single scan pass.
/// This avoids re-compiling the same regex string on every file.
struct CompiledFilters {
    excluded_extensions: Vec<String>,
    regex_patterns: Vec<regex::Regex>,
}

impl CompiledFilters {
    fn from_config(config: &AppConfig) -> Self {
        let regex_patterns = config
            .regex_paths
            .iter()
            .filter_map(|p| regex::Regex::new(p).ok())
            .collect();

        Self {
            excluded_extensions: config.excluded_extensions.clone(),
            regex_patterns,
        }
    }

    fn should_ignore(&self, path: &Path, base_dir: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // 1. Extension/name check
        for ext in &self.excluded_extensions {
            if file_name.ends_with(ext.as_str()) {
                return true;
            }
        }

        // 2. Regex check against relative path
        // IMPORTANT: We test both `rel` and `rel/` (with trailing slash).
        // Patterns like `^node_modules/.*` require a slash after the directory
        // name. Without the trailing slash, the directory entry `node_modules`
        // itself would not be pruned — only files inside it would match.
        // `.*` matches the empty string so `node_modules/` satisfies `^node_modules/.*`.
        if !self.regex_patterns.is_empty() {
            if let Ok(relative) = path.strip_prefix(base_dir) {
                let rel_str = relative.to_string_lossy();
                let rel_slash = format!("{}/", rel_str);
                for re in &self.regex_patterns {
                    if re.is_match(&rel_str) || re.is_match(&rel_slash) {
                        return true;
                    }
                }
            }
        }

        false
    }
}

/// Parallel recursive scan using rayon work-stealing.
/// Each subdirectory is dispatched as a separate rayon task so all CPU
/// cores are utilised. Atomic counters replace the mutable references
/// that forced the old implementation to stay single-threaded.
fn scan_dir_parallel(
    path: &Path,
    base_dir: &Path,
    filters: &CompiledFilters,
    file_count: &Arc<AtomicU64>,
    total_bytes: &Arc<AtomicU64>,
    app: &AppHandle,
) {
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Separate files and subdirectories in one pass so we can then
    // send subdirs to rayon::scope without holding read_dir open.
    let mut sub_dirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let entry_path = entry.path();

        if filters.should_ignore(&entry_path, base_dir) {
            continue;
        }

        match entry.file_type() {
            Ok(ft) if ft.is_dir() => sub_dirs.push(entry_path),
            Ok(ft) if ft.is_file() => {
                let count = file_count.fetch_add(1, Ordering::Relaxed) + 1;
                if let Ok(meta) = entry.metadata() {
                    total_bytes.fetch_add(meta.len(), Ordering::Relaxed);
                }

                // Emit UI progress every 50 files
                if count % 50 == 0 {
                    let _ = app.emit("sync-scan-progress", ScanProgress {
                        file_count: count,
                        total_bytes: total_bytes.load(Ordering::Relaxed),
                        is_done: false,
                    });
                }
            }
            _ => {}
        }
    }

    // Recurse into subdirectories in parallel
    sub_dirs.par_iter().for_each(|sub| {
        scan_dir_parallel(sub, base_dir, filters, file_count, total_bytes, app);
    });
}

pub fn should_ignore(
    path: &Path,
    base_dir: &Path,
    config: &AppConfig,
) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    for ext in &config.excluded_extensions {
        if file_name.ends_with(ext.as_str()) {
            return true;
        }
    }

    // Test both `rel` and `rel/` — see CompiledFilters::should_ignore for rationale.
    if let Ok(relative_path) = path.strip_prefix(base_dir) {
        let rel_str = relative_path.to_string_lossy();
        let rel_slash = format!("{}/", rel_str);
        for pattern in &config.regex_paths {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(&rel_str) || re.is_match(&rel_slash) {
                    return true;
                }
            }
        }
    }

    false
}

#[tauri::command]
pub fn start_local_scan(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        // Load configurations
        let config = match crate::config::load_config(app.clone()) {
            Ok(c) => c,
            Err(_) => return,
        };

        // Pre-compile all regex patterns once for the entire scan
        let filters = Arc::new(CompiledFilters::from_config(&config));

        // Shared atomic counters — safe to read/write from rayon threads
        let file_count = Arc::new(AtomicU64::new(0));
        let total_bytes = Arc::new(AtomicU64::new(0));

        // Emit initial scan state
        let _ = app.emit("sync-scan-progress", ScanProgress {
            file_count: 0,
            total_bytes: 0,
            is_done: false,
        });

        // Collect valid root folders first so we can parallelise at the top level too
        let root_paths: Vec<PathBuf> = config
            .sync_folders
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.exists() && p.is_dir())
            .collect();

        // Run the parallel scan on a rayon blocking thread so the tokio runtime
        // is not blocked during the CPU-intensive traversal.
        let file_count_clone = file_count.clone();
        let total_bytes_clone = total_bytes.clone();
        let filters_clone = filters.clone();
        let app_clone = app.clone();

        tokio::task::spawn_blocking(move || {
            root_paths.par_iter().for_each(|root| {
                scan_dir_parallel(
                    root,
                    root,
                    &filters_clone,
                    &file_count_clone,
                    &total_bytes_clone,
                    &app_clone,
                );
            });
        }).await.ok();

        // Emit final scan state
        let _ = app.emit("sync-scan-progress", ScanProgress {
            file_count: file_count.load(Ordering::Relaxed),
            total_bytes: total_bytes.load(Ordering::Relaxed),
            is_done: true,
        });
    });

    Ok(())
}

pub struct WatcherState {
    pub watcher: std::sync::Mutex<Option<notify::RecommendedWatcher>>,
}

/// Removes the given file paths from sync_logs so they are treated as
/// "unsynced" (dirty) on the next sync run.
fn mark_files_dirty(app: &AppHandle, paths: Vec<std::path::PathBuf>) {
    let db_path = crate::db::get_db_path(app);
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let _ = conn.busy_timeout(std::time::Duration::from_millis(3000));
        for path in paths {
            let path_str = path.to_string_lossy().to_string();
            let _ = conn.execute(
                "DELETE FROM sync_logs WHERE file_path = ?",
                [&path_str],
            );
            println!("Marked as dirty (will re-sync): {}", path_str);
        }
    }
}

#[tauri::command]
pub fn setup_watcher(app: AppHandle) -> Result<(), String> {
    use notify::Watcher;

    // Load config
    let config = crate::config::load_config(app.clone())?;

    // Get managed WatcherState
    let state = app.state::<WatcherState>();
    let mut watcher_guard = state.watcher.lock().map_err(|e| e.to_string())?;

    // Drop old watcher (stops watching automatically)
    *watcher_guard = None;

    if config.sync_folders.is_empty() {
        return Ok(());
    }

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::RecommendedWatcher::new(tx, notify::Config::default()).map_err(|e| e.to_string())?;

    // Watch folders recursively
    for folder_str in &config.sync_folders {
        let path = PathBuf::from(folder_str);
        if path.exists() && path.is_dir() {
            let _ = watcher.watch(&path, notify::RecursiveMode::Recursive);
            println!("FileSystem Watcher started watching: {:?}", path);
        }
    }

    // Spawn background thread to handle events with debouncing
    let app_clone = app.clone();
    std::thread::spawn(move || {
        let debounce_duration = std::time::Duration::from_millis(800);
        loop {
            // Wait for the first event
            match rx.recv() {
                Ok(Ok(first_event)) => {
                    // Collect affected paths from this first event
                    let mut changed_paths: Vec<PathBuf> = first_event.paths.clone();

                    loop {
                        match rx.recv_timeout(debounce_duration) {
                            Ok(Ok(ev)) => {
                                // Accumulate paths from additional events
                                for p in ev.paths {
                                    if !changed_paths.contains(&p) {
                                        changed_paths.push(p);
                                    }
                                }
                            }
                            Ok(Err(_)) => {}
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                break;
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                return;
                            }
                        }
                    }

                    // Only keep files (not directories)
                    let file_paths: Vec<PathBuf> = changed_paths
                        .into_iter()
                        .filter(|p| p.is_file())
                        .collect();

                    if !file_paths.is_empty() {
                        println!(
                            "Filesystem changed ({} files), marking dirty and triggering re-sync...",
                            file_paths.len()
                        );
                        // Mark changed files as dirty so next sync re-uploads them
                        mark_files_dirty(&app_clone, file_paths);
                    }

                    // Trigger directory re-scan on file system quiet period
                    let _ = start_local_scan(app_clone.clone());

                    // Trigger synchronization worker automatically
                    let app_for_sync = app_clone.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::sync::run_sync_process(app_for_sync).await;
                    });
                }
                Ok(Err(_)) => {}
                Err(_) => return, // Channel disconnected
            }
        }
    });

    // Store new watcher in tauri managed state
    *watcher_guard = Some(watcher);

    Ok(())
}

pub fn collect_files_recursive(
    dir_path: &Path,
    base_dir: &Path,
    config: &AppConfig,
    files: &mut Vec<(PathBuf, PathBuf)>,
) {
    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();

                // Skip ignored directories/files immediately!
                if should_ignore(&path, base_dir, config) {
                    continue;
                }

                if path.is_dir() {
                    collect_files_recursive(&path, base_dir, config, files);
                } else if path.is_file() {
                    files.push((path, base_dir.to_path_buf()));
                }
            }
        }
    }
}
