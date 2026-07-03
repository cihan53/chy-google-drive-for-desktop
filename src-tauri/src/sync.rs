use tauri::{AppHandle, Emitter};
use std::sync::{OnceLock, Mutex, Arc};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Global pause flag — set to `true` to pause an in-progress sync.
pub static SYNC_PAUSED: OnceLock<AtomicBool> = OnceLock::new();

pub fn get_pause_flag() -> &'static AtomicBool {
    SYNC_PAUSED.get_or_init(|| AtomicBool::new(false))
}
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, serde::Serialize)]
struct SyncStartedPayload {
    total: u64,
}

#[derive(Clone, serde::Serialize)]
struct SyncProgressPayload {
    completed: u64,
    total: u64,
}

pub static CURRENT_SYNC_CANCEL: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();

pub fn get_cancel_flag() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    CURRENT_SYNC_CANCEL.get_or_init(|| Mutex::new(None))
}

pub struct SyncCancelGuard {
    app: AppHandle,
}

impl Drop for SyncCancelGuard {
    fn drop(&mut self) {
        if let Some(lock) = CURRENT_SYNC_CANCEL.get() {
            let mut guard = lock.lock().unwrap();
            *guard = None;
        }
        // Clean up database sync logs that were left in_progress
        let _ = crate::db::clean_in_progress_logs(&self.app);
        // Inform frontend to refresh logs/status
        let _ = self.app.emit("sync-status-update", ());
    }
}

async fn get_or_create_backup_folder(app: &AppHandle, access_token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();

    // 1. Query for existing folder
    let url = "https://www.googleapis.com/drive/v3/files?q=name%20%3D%20%27Chy%20Drive%20Backup%27%20and%20mimeType%20%3D%20%27application%2Fvnd.google-apps.folder%27%20and%20trashed%20%3D%20false";
    let mut retries = 3;
    let mut delay = std::time::Duration::from_millis(500);
    let mut res;

    loop {
        let req = client.get(url).bearer_auth(access_token);
        res = req.send().await;

        match &res {
            Ok(response) if response.status().is_success() => break,
            Ok(response) if response.status().is_server_error() && retries > 0 => {
                println!("Query backup folder returned 5xx, retrying in {:?}... (retries left: {})", delay, retries);
                tokio::time::sleep(delay).await;
                retries -= 1;
                delay *= 2;
            }
            _ => break,
        }
    }

    let res = res.map_err(|e| e.to_string())?;
    let status = res.status();
    if status.is_success() {
        let val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        if let Some(files) = val.get("files").and_then(|f| f.as_array()) {
            if !files.is_empty() {
                if let Some(id) = files[0].get("id").and_then(|i| i.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }
    } else {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let _ = crate::auth::delete_token(app);
        }
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Query backup folder failed (HTTP {}): {}", status, err_text));
    }

    // 2. Not found, create it
    let body = serde_json::json!({
        "name": "Chy Drive Backup",
        "mimeType": "application/vnd.google-apps.folder"
    });

    let mut retries = 3;
    let mut delay = std::time::Duration::from_millis(500);
    let mut res;

    loop {
        let req = client.post("https://www.googleapis.com/drive/v3/files")
            .bearer_auth(access_token)
            .json(&body);
        res = req.send().await;

        match &res {
            Ok(response) if response.status().is_success() => break,
            Ok(response) if response.status().is_server_error() && retries > 0 => {
                println!("Create backup folder returned 5xx, retrying in {:?}... (retries left: {})", delay, retries);
                tokio::time::sleep(delay).await;
                retries -= 1;
                delay *= 2;
            }
            _ => break,
        }
    }

    let res = res.map_err(|e| e.to_string())?;
    let status = res.status();
    if status.is_success() {
        let val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        if let Some(id) = val.get("id").and_then(|i| i.as_str()) {
            return Ok(id.to_string());
        }
        Err("Folder created but no ID returned".to_string())
    } else {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let _ = crate::auth::delete_token(app);
        }
        let err_text = res.text().await.unwrap_or_default();
        Err(format!("Create backup folder failed (HTTP {}): {}", status, err_text))
    }
}

async fn get_or_create_drive_subfolder(
    app: &AppHandle,
    access_token: &str,
    parent_id: &str,
    name: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();

    // Query format: name = 'name' and mimeType = 'application/vnd.google-apps.folder' and 'parent_id' in parents and trashed = false
    let query = format!(
        "name = '{}' and mimeType = 'application/vnd.google-apps.folder' and '{}' in parents and trashed = false",
        name.replace("'", "\\'"),
        parent_id
    );

    let encoded_query: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}",
        encoded_query
    );

    let mut retries = 3;
    let mut delay = std::time::Duration::from_millis(500);
    let mut res;

    loop {
        let req = client.get(&url).bearer_auth(access_token);
        res = req.send().await;

        match &res {
            Ok(response) if response.status().is_success() => break,
            Ok(response) if response.status().is_server_error() && retries > 0 => {
                println!("Query drive subfolder '{}' returned 5xx, retrying in {:?}... (retries left: {})", name, delay, retries);
                tokio::time::sleep(delay).await;
                retries -= 1;
                delay *= 2;
            }
            _ => break,
        }
    }

    let res = res.map_err(|e| e.to_string())?;
    let status = res.status();
    if status.is_success() {
        let val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        if let Some(files) = val.get("files").and_then(|f| f.as_array()) {
            if !files.is_empty() {
                if let Some(id) = files[0].get("id").and_then(|i| i.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }
    } else {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let _ = crate::auth::delete_token(app);
        }
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Query folder '{}' failed (HTTP {}): {}", name, status, err_text));
    }

    // Create folder
    let body = serde_json::json!({
        "name": name,
        "mimeType": "application/vnd.google-apps.folder",
        "parents": [parent_id]
    });

    let mut retries = 3;
    let mut delay = std::time::Duration::from_millis(500);
    let mut res;

    loop {
        let req = client.post("https://www.googleapis.com/drive/v3/files")
            .bearer_auth(access_token)
            .json(&body);
        res = req.send().await;

        match &res {
            Ok(response) if response.status().is_success() => break,
            Ok(response) if response.status().is_server_error() && retries > 0 => {
                println!("Create drive subfolder '{}' returned 5xx, retrying in {:?}... (retries left: {})", name, delay, retries);
                tokio::time::sleep(delay).await;
                retries -= 1;
                delay *= 2;
            }
            _ => break,
        }
    }

    let res = res.map_err(|e| e.to_string())?;
    let status = res.status();
    if status.is_success() {
        let val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        if let Some(id) = val.get("id").and_then(|i| i.as_str()) {
            return Ok(id.to_string());
        }
    } else {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let _ = crate::auth::delete_token(app);
        }
    }

    Err(format!("Failed to get or create folder: {}", name))
}

async fn resolve_parent_folder_id(
    app: &AppHandle,
    access_token: &str,
    base_parent_id: &str,
    relative_path: &Path,
    cache: &mut HashMap<String, String>,
) -> Result<String, String> {
    let parent_dir = match relative_path.parent() {
        Some(p) => p,
        None => return Ok(base_parent_id.to_string()),
    };

    let mut components = Vec::new();
    for component in parent_dir.components() {
        if let std::path::Component::Normal(name) = component {
            if let Some(name_str) = name.to_str() {
                components.push(name_str);
            }
        }
    }

    if components.is_empty() {
        return Ok(base_parent_id.to_string());
    }

    let mut current_parent_id = base_parent_id.to_string();
    let mut current_path = Vec::new();

    for folder_name in components {
        current_path.push(folder_name);
        let cache_key = current_path.join("/");

        if let Some(cached_id) = cache.get(&cache_key) {
            current_parent_id = cached_id.clone();
        } else {
            // Find or create the folder
            let new_id = get_or_create_drive_subfolder(app, access_token, &current_parent_id, folder_name).await?;
            cache.insert(cache_key, new_id.clone());
            current_parent_id = new_id;
        }
    }

    Ok(current_parent_id)
}

async fn upload_file_to_drive(app: &AppHandle, access_token: &str, parent_id: &str, file_path: &Path) -> Result<(), String> {
    let file_name = file_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid file name".to_string())?;

    let file_bytes = std::fs::read(file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let client = reqwest::Client::new();
    let boundary = "foo_bar_upload_boundary";

    let metadata = serde_json::json!({
        "name": file_name,
        "parents": [parent_id]
    });

    let metadata_part = format!(
        "--{}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{}\r\n",
        boundary,
        metadata.to_string()
    );

    let file_header = format!(
        "--{}\r\nContent-Type: application/octet-stream\r\n\r\n",
        boundary
    );

    let file_footer = format!(
        "\r\n--{}--\r\n",
        boundary
    );

    let mut body = Vec::new();
    body.extend_from_slice(metadata_part.as_bytes());
    body.extend_from_slice(file_header.as_bytes());
    body.extend_from_slice(&file_bytes);
    body.extend_from_slice(file_footer.as_bytes());

    let mut retries = 3;
    let mut delay = std::time::Duration::from_millis(1000);
    let mut res;

    loop {
        let req = client.post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
            .bearer_auth(access_token)
            .header("Content-Type", format!("multipart/related; boundary={}", boundary))
            .body(body.clone());
        res = req.send().await;

        match &res {
            Ok(response) if response.status().is_success() => break,
            Ok(response) if response.status().is_server_error() && retries > 0 => {
                println!("Upload file returned 5xx, retrying in {:?}... (retries left: {})", delay, retries);
                tokio::time::sleep(delay).await;
                retries -= 1;
                delay *= 2;
            }
            _ => break,
        }
    }

    let res = res.map_err(|e| e.to_string())?;
    let status = res.status();
    if status.is_success() {
        Ok(())
    } else {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let _ = crate::auth::delete_token(app);
        }
        let err_text = res.text().await.unwrap_or_default();
        Err(format!("Upload file failed (HTTP {}): {}", status, err_text))
    }
}

/// Loads all successfully synced file paths from DB into a HashSet in a single query.
/// This replaces per-file `is_file_synced` calls (N queries → 1 query + O(1) lookups).
fn load_synced_paths(conn: &rusqlite::Connection) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT file_path FROM sync_logs WHERE status = 'success'"
    ) {
        let _ = stmt.query_map([], |row| row.get::<_, String>(0))
            .map(|rows| {
                for row in rows.flatten() {
                    set.insert(row);
                }
            });
    }
    set
}

fn log_sync_event(conn: &rusqlite::Connection, file_path: &str, action: &str, status: &str) {
    let _ = conn.execute(
        "INSERT INTO sync_logs (file_path, action, status) VALUES (?1, ?2, ?3)",
        [file_path, action, status],
    );
}

pub async fn run_sync_process(app: AppHandle) -> Result<(), String> {
    println!("Starting synchronization process...");
    let result = run_sync_process_internal(app.clone()).await;

    match &result {
        Ok(_) => {
            println!("Sync process completed successfully.");
        }
        Err(e) => {
            println!("Sync process failed: {}", e);
            let _ = app.emit("sync-error", e.clone());
        }
    }

    let _ = app.emit("sync-completed", ());
    result
}

async fn run_sync_process_internal(app: AppHandle) -> Result<(), String> {
    // 1. Get access token
    let access_token = match crate::auth::retrieve_token(&app) {
        Ok(t) => t,
        Err(_) => {
            println!("Sync skipped: User not logged in.");
            return Ok(());
        }
    };

    // Notify frontend that we are in the "checking / scanning" phase
    let _ = app.emit("sync-checking", ());

    // 2. Get or create backup root folder ID in Google Drive
    let root_parent_id = match get_or_create_backup_folder(&app, &access_token).await {
        Ok(id) => id,
        Err(e) => {
            println!("Sync error (root folder): {}", e);
            return Err(e);
        }
    };

    // Get the machine's name (hostname)
    let machine_name = get_machine_name();
    println!("Local machine name resolved to: {}", machine_name);

    // Get or create device-specific subfolder under the root backup folder
    let parent_id = match get_or_create_drive_subfolder(&app, &access_token, &root_parent_id, &machine_name).await {
        Ok(id) => id,
        Err(e) => {
            println!("Sync error (machine folder): {}", e);
            return Err(e);
        }
    };

    // 3. Load configurations
    let config = match crate::config::load_config(app.clone()) {
        Ok(c) => c,
        Err(e) => return Err(e),
    };

    // 4. Open connection to SQLite — wrap in Arc<Mutex> immediately so it can
    //    be shared between the spawn_blocking file-scan and the upload tasks.
    let db_path = crate::db::get_db_path(&app);
    let conn_raw = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn_raw.busy_timeout(std::time::Duration::from_millis(5000)).map_err(|e| e.to_string())?;
    let conn = std::sync::Arc::new(std::sync::Mutex::new(conn_raw));

    // 5. Collect all files to sync — run on a blocking thread so the
    //    tokio runtime is not stalled during filesystem traversal.
    let config_clone = config.clone();
    let conn_scan = conn.clone();
    let (files, synced_paths) = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        for folder_str in &config_clone.sync_folders {
            let path = PathBuf::from(folder_str);
            if path.exists() && path.is_dir() {
                crate::local_fs::collect_files_recursive(&path, &path, &config_clone, &mut files);
            }
        }
        let synced_paths = {
            let conn_guard = conn_scan.lock().unwrap();
            load_synced_paths(&conn_guard)
        };
        (files, synced_paths)
    }).await.map_err(|e| e.to_string())?;

    // Filter unsynced files
    let unsynced_files: Vec<(PathBuf, PathBuf)> = files
        .into_iter()
        .filter(|(path, _)| !synced_paths.contains(&path.to_string_lossy().to_string()))
        .collect();

    if unsynced_files.is_empty() {
        println!("All files are up to date.");
        return Ok(());
    }

    let session_total = unsynced_files.len() as u64;
    println!("Total unsynced files found: {}", session_total);
    // Inform frontend of total count for this session so the progress bar
    // can show meaningful per-session progress instead of all-time ratio.
    let _ = app.emit("sync-started", SyncStartedPayload { total: session_total });


    // Initialize folder cache using an Arc-Mutex for safe concurrent resolution
    let folder_cache = std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // Limit concurrency to 4 uploads to prevent Google Drive API rate limits
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let is_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Session-level completed counter shared across all upload tasks
    let session_completed = Arc::new(AtomicU64::new(0));

    // Set as the current sync cancellation flag and setup the drop guard
    let _cancel_guard = SyncCancelGuard { app: app.clone() };
    {
        let mut lock = get_cancel_flag().lock().unwrap();
        *lock = Some(is_cancelled.clone());
    }

    // Determine chunk size: usize::MAX means "no throttle" (one big batch)
    let chunk_size = if config.throttle_enabled && config.throttle_chunk_size > 0 {
        config.throttle_chunk_size as usize
    } else {
        usize::MAX
    };
    let interval_secs = config.throttle_interval_secs;

    let parent_id_arc = std::sync::Arc::new(parent_id.clone());

    let mut chunk_start = 0;
    while chunk_start < unsynced_files.len() {
        if is_cancelled.load(Ordering::Relaxed) {
            break;
        }

        // Wait while paused before starting each chunk
        while get_pause_flag().load(Ordering::Relaxed) {
            if is_cancelled.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        if is_cancelled.load(Ordering::Relaxed) {
            break;
        }

        let chunk_end = (chunk_start + chunk_size).min(unsynced_files.len());
        let chunk = &unsynced_files[chunk_start..chunk_end];

        let mut handles = Vec::new();

        for (path, base_dir) in chunk {
            let sem = semaphore.clone();
            let app_clone = app.clone();
            let access_token_clone = access_token.clone();
            let conn_clone = conn.clone();
            let folder_cache_clone = folder_cache.clone();
            let parent_id_str = parent_id_arc.as_ref().clone();
            let is_cancelled_clone = is_cancelled.clone();
            let session_completed_clone = session_completed.clone();
            let path = path.clone();
            let base_dir = base_dir.clone();

            let handle = tauri::async_runtime::spawn(async move {
                if is_cancelled_clone.load(Ordering::Relaxed) {
                    return;
                }

                // Wait while sync is paused (check cancel every 200 ms)
                while get_pause_flag().load(Ordering::Relaxed) {
                    if is_cancelled_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }

                let file_path_str = path.to_string_lossy().to_string();

                // Get relative path of the file to the base directory
                let relative_path = match path.strip_prefix(&base_dir) {
                    Ok(p) => p.to_path_buf(),
                    Err(_) => path.clone(),
                };

                // Resolve parent folder ID inside Google Drive
                let parent_folder_id = {
                    let mut cache_guard = folder_cache_clone.lock().await;
                    if is_cancelled_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    match resolve_parent_folder_id(&app_clone, &access_token_clone, &parent_id_str, &relative_path, &mut cache_guard).await {
                        Ok(id) => id,
                        Err(e) => {
                            println!("Failed to resolve folder path for {}: {}", file_path_str, e);
                            if e.contains("401") || e.contains("Unauthorized") || e.contains("UNAUTHENTICATED") {
                                is_cancelled_clone.store(true, Ordering::Relaxed);
                            }
                            {
                                let conn_guard = conn_clone.lock().unwrap();
                                log_sync_event(&conn_guard, &file_path_str, "upload", "error");
                            }
                            let _ = app_clone.emit("sync-status-update", ());
                            return;
                        }
                    }
                };

                if is_cancelled_clone.load(Ordering::Relaxed) {
                    return;
                }

                // Acquire semaphore permit for upload
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(e) => {
                        println!("Failed to acquire semaphore permit: {:?}", e);
                        return;
                    }
                };

                if is_cancelled_clone.load(Ordering::Relaxed) {
                    return;
                }

                // Log upload status as in_progress
                {
                    let conn_guard = conn_clone.lock().unwrap();
                    log_sync_event(&conn_guard, &file_path_str, "upload", "in_progress");
                }
                let _ = app_clone.emit("sync-status-update", ());

                // Upload the file
                println!("Syncing file: {} (parent: {})", file_path_str, parent_folder_id);
                let result = upload_file_to_drive(&app_clone, &access_token_clone, &parent_folder_id, &path).await;

                // Log final upload status (success or error)
                {
                    let conn_guard = conn_clone.lock().unwrap();
                    match &result {
                        Ok(_) => {
                            println!("Successfully synced: {}", file_path_str);
                            log_sync_event(&conn_guard, &file_path_str, "upload", "success");
                        }
                        Err(e) => {
                            println!("Failed to sync {}: {}", file_path_str, e);
                            if e.contains("401") || e.contains("Unauthorized") || e.contains("UNAUTHENTICATED") {
                                is_cancelled_clone.store(true, Ordering::Relaxed);
                            }
                            log_sync_event(&conn_guard, &file_path_str, "upload", "error");
                        }
                    }
                }

                // Increment session counter and emit real-time progress
                let completed = session_completed_clone.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = app_clone.emit("sync-progress", SyncProgressPayload {
                    completed,
                    total: session_total,
                });
                let _ = app_clone.emit("sync-status-update", ());
            });
            handles.push(handle);
        }

        // Wait for this chunk to complete
        for handle in handles {
            let _ = handle.await;
        }

        chunk_start = chunk_end;

        // If throttle is active and there are more files, sleep before next chunk
        if config.throttle_enabled && chunk_start < unsynced_files.len() && !is_cancelled.load(Ordering::Relaxed) {
            println!("Throttle: waiting {} seconds before next chunk...", interval_secs);
            let sleep_dur = std::time::Duration::from_secs(interval_secs as u64);
            let check_interval = std::time::Duration::from_millis(200);
            let mut elapsed = std::time::Duration::ZERO;
            while elapsed < sleep_dur {
                if is_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                tokio::time::sleep(check_interval).await;
                elapsed += check_interval;
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn trigger_sync(app: AppHandle) -> Result<String, String> {
    // Clear any leftover paused state before starting a fresh sync
    get_pause_flag().store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn(async move {
        let _ = run_sync_process(app).await;
    });
    Ok("Sync triggered".to_string())
}

#[tauri::command]
pub async fn pause_sync() -> Result<(), String> {
    get_pause_flag().store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn resume_sync() -> Result<(), String> {
    get_pause_flag().store(false, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn check_and_run_auto_sync(app: AppHandle) -> Result<bool, String> {
    // 1. Check if logged in
    if crate::auth::retrieve_token(&app).is_err() {
        return Ok(false);
    }

    // 2. Check if any sync folders configured
    let config = crate::config::load_config(app.clone())?;
    if config.sync_folders.is_empty() {
        return Ok(false);
    }

    // 3. Spawn the full sync process — it will emit `sync-checking` while
    //    scanning, then `sync-started` with total count once it knows how
    //    many files need uploading, and exit early if everything is up to date.
    //    No file scanning is done here to avoid blocking the tokio runtime.
    println!("Auto-sync: Spawning background sync check...");
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = run_sync_process(app_clone).await;
    });
    Ok(true)
}

fn get_machine_name() -> String {
    if let Ok(output) = std::process::Command::new("hostname").output() {
        if let Ok(name) = String::from_utf8(output.stdout) {
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    // Fallbacks
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "Unknown-Device".to_string())
}
