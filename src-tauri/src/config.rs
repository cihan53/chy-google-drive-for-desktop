use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// A single scheduled backup window.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleEntry {
    /// Whether this schedule is active.
    pub enabled: bool,
    /// Days of the week: 0 = Sunday … 6 = Saturday
    pub days: Vec<u8>,
    pub hour: u8,
    pub minute: u8,
}

impl Default for ScheduleEntry {
    fn default() -> Self {
        Self {
            enabled: false,
            days: vec![1, 2, 3, 4, 5], // Mon-Fri
            hour: 23,
            minute: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub excluded_extensions: Vec<String>,
    pub regex_paths: Vec<String>,
    pub sync_folders: Vec<String>,
    pub bandwidth_limit_enabled: bool,
    pub max_upload_rate_kb: u32,
    pub max_download_rate_kb: u32,
    /// When true, uploads are spread over time in chunks.
    pub throttle_enabled: bool,
    /// Number of files uploaded per chunk (default 10).
    pub throttle_chunk_size: u32,
    /// Seconds to wait between chunks (default 5).
    pub throttle_interval_secs: u32,
    /// Optional recurring backup schedule.
    pub schedule: Option<ScheduleEntry>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            excluded_extensions: vec![".tmp".to_string(), ".log".to_string(), ".DS_Store".to_string()],
            regex_paths: vec!["^node_modules/.*".to_string()],
            sync_folders: vec![],
            bandwidth_limit_enabled: false,
            max_upload_rate_kb: 300,
            max_download_rate_kb: 1024,
            throttle_enabled: false,
            throttle_chunk_size: 10,
            throttle_interval_secs: 5,
            schedule: None,
        }
    }
}

fn get_config_path(app: &AppHandle) -> PathBuf {
    let mut path = app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."));
    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path.push("config.json");
    path
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> Result<AppConfig, String> {
    let config_path = get_config_path(&app);
    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(raw_val) = serde_json::from_str::<serde_json::Value>(&content) {
                if raw_val.is_object() {
                    let mut final_config = AppConfig::default();
                    let obj = raw_val.as_object().unwrap();

                    if let Some(exts) = obj.get("excluded_extensions").and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()) {
                        final_config.excluded_extensions = exts;
                    }
                    if let Some(paths) = obj.get("regex_paths").and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()) {
                        final_config.regex_paths = paths;
                    }

                    // Migrate sync_folder (old) to sync_folders (new)
                    if let Some(folders) = obj.get("sync_folders").and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()) {
                        final_config.sync_folders = folders;
                    } else if let Some(folder) = obj.get("sync_folder").and_then(|v| v.as_str()) {
                        if !folder.is_empty() {
                            final_config.sync_folders = vec![folder.to_string()];
                        }
                    }

                    if let Some(bw) = obj.get("bandwidth_limit_enabled").and_then(|v| v.as_bool()) {
                        final_config.bandwidth_limit_enabled = bw;
                    }
                    if let Some(up) = obj.get("max_upload_rate_kb").and_then(|v| v.as_u64()) {
                        final_config.max_upload_rate_kb = up as u32;
                    }
                    if let Some(down) = obj.get("max_download_rate_kb").and_then(|v| v.as_u64()) {
                        final_config.max_download_rate_kb = down as u32;
                    }

                    // Throttle settings (graceful migration — defaults if absent)
                    if let Some(t) = obj.get("throttle_enabled").and_then(|v| v.as_bool()) {
                        final_config.throttle_enabled = t;
                    }
                    if let Some(cs) = obj.get("throttle_chunk_size").and_then(|v| v.as_u64()) {
                        final_config.throttle_chunk_size = cs as u32;
                    }
                    if let Some(iv) = obj.get("throttle_interval_secs").and_then(|v| v.as_u64()) {
                        final_config.throttle_interval_secs = iv as u32;
                    }

                    // Schedule (graceful migration)
                    if let Some(sched) = obj.get("schedule").and_then(|v| serde_json::from_value::<ScheduleEntry>(v.clone()).ok()) {
                        final_config.schedule = Some(sched);
                    }

                    return Ok(final_config);
                }
            }
        }
    }

    let default_config = AppConfig::default();
    let _ = save_config(app, default_config.clone());
    Ok(default_config)
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    let config_path = get_config_path(&app);
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(config_path, content).map_err(|e| e.to_string())?;

    // 1. Cancel any active sync run
    {
        if let Some(lock) = crate::sync::CURRENT_SYNC_CANCEL.get() {
            let mut guard = lock.lock().unwrap();
            if let Some(cancel_flag) = guard.as_ref() {
                cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                println!("Sync cancelled due to settings update.");
            }
            *guard = None;
        }
    }

    // 2. Setup the folder watcher again
    let _ = crate::local_fs::setup_watcher(app.clone());

    // 3. Trigger auto sync in the background with the new settings
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        println!("Restarting sync process with new configurations...");
        let _ = crate::sync::check_and_run_auto_sync(app).await;
    });

    Ok(())
}
