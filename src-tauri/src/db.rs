use rusqlite::{Connection, Result};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn get_db_path(app: &AppHandle) -> PathBuf {
    let mut path = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path.push("sync_logs.sqlite");
    path
}

pub fn init_db(app: &AppHandle) -> Result<()> {
    let db_path = get_db_path(app);
    let conn = Connection::open(&db_path)?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path TEXT NOT NULL,
            action TEXT NOT NULL,
            status TEXT NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    // Create index on file_path to significantly speed up file check lookups
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_sync_logs_file_path ON sync_logs (file_path)",
        [],
    )?;

    Ok(())
}

#[tauri::command]
pub fn get_recent_logs(app: AppHandle) -> Result<Vec<String>, String> {
    let db_path = get_db_path(&app);
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(5000)).map_err(|e| e.to_string())?;

    let mut stmt = conn.prepare("SELECT file_path, action, status, timestamp FROM sync_logs ORDER BY id DESC LIMIT 50").map_err(|e| e.to_string())?;
    
    let logs_iter = stmt.query_map([], |row| {
        let file_path: String = row.get(0)?;
        let action: String = row.get(1)?;
        let status: String = row.get(2)?;
        let timestamp: String = row.get(3)?;
        Ok(format!("[{}] {} {} - {}", timestamp, action, file_path, status))
    }).map_err(|e| e.to_string())?;

    let mut logs = Vec::new();
    for log in logs_iter {
        logs.push(log.unwrap_or_else(|_| "Error reading row".to_string()));
    }

    Ok(logs)
}

#[tauri::command]
pub fn get_synced_files_count(app: AppHandle) -> Result<i64, String> {
    let db_path = get_db_path(&app);
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(5000)).map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT COUNT(DISTINCT file_path) FROM sync_logs WHERE status = 'success'")
        .map_err(|e| e.to_string())?;

    let count: i64 = stmt.query_row([], |row| row.get(0)).map_err(|e| e.to_string())?;
    Ok(count)
}

#[tauri::command]
pub fn clear_sync_logs(app: AppHandle) -> Result<(), String> {
    let db_path = get_db_path(&app);
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(5000)).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM sync_logs", []).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clean_in_progress_logs(app: &AppHandle) -> Result<(), String> {
    let db_path = get_db_path(app);
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(5000)).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM sync_logs WHERE status = 'in_progress'",
        [],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

