mod auth;
mod config;
mod db;
mod local_fs;
mod scheduler;
mod sync;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
    WindowEvent,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_dialog::init())
    .on_window_event(|window, event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            let _ = window.hide();
            api.prevent_close();
        }
        _ => {}
    })
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      
      let _ = db::init_db(app.handle());

      app.manage(local_fs::WatcherState {
          watcher: std::sync::Mutex::new(None),
      });
      let _ = local_fs::setup_watcher(app.handle().clone());

      let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
      let show_i = MenuItem::with_id(app, "show", "Show Settings", true, None::<&str>)?;
      let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

      let _tray = TrayIconBuilder::new()
          .menu(&menu)
          .on_menu_event(|app, event| match event.id.as_ref() {
              "quit" => {
                  app.exit(0);
              }
              "show" => {
                  if let Some(window) = app.get_webview_window("main") {
                      let _ = window.show();
                      let _ = window.set_focus();
                  }
              }
              _ => {}
          })
          .build(app)?;

      // Start background scheduler (checks every 30s if a sync is due)
      scheduler::start_scheduler(app.handle().clone());

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
        config::load_config,
        config::save_config,
        db::get_recent_logs,
        db::get_synced_files_count,
        db::clear_sync_logs,
        auth::login_with_google,
        auth::check_auth_status,
        auth::logout,
        auth::get_user_profile,
        auth::get_storage_quota,
        local_fs::start_local_scan,
        local_fs::setup_watcher,
        sync::trigger_sync,
        sync::pause_sync,
        sync::resume_sync,
        sync::check_and_run_auto_sync
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
