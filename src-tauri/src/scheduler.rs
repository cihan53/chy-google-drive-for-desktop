use chrono::{Datelike, Timelike};
use tauri::AppHandle;

/// Starts the background scheduler loop.
///
/// Every 60 seconds the loop wakes up, loads the current config, and checks
/// whether the current local time matches any enabled schedule entry.
/// A match triggers a full sync (identical to `check_and_run_auto_sync`).
///
/// The loop exits if the task is cancelled externally (process exits).
pub fn start_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        println!("Scheduler: background loop started.");
        // Track the last minute we fired on to avoid double-triggering
        // within the same minute.
        let mut last_fired_minute: Option<(u8, u8)> = None; // (hour, minute)

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            // Load config to get current schedule
            let config = match crate::config::load_config(app.clone()) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let sched = match &config.schedule {
                Some(s) if s.enabled => s.clone(),
                _ => {
                    last_fired_minute = None;
                    continue;
                }
            };

            // Get local time
            let now = chrono::Local::now();
            let current_hour = now.hour() as u8;
            let current_minute = now.minute() as u8;
            // chrono weekday: Mon=0 … Sun=6; we use Sun=0 … Sat=6 convention
            // to match JS getDay() so the UI stays consistent.
            let weekday_js = match now.weekday() {
                chrono::Weekday::Sun => 0u8,
                chrono::Weekday::Mon => 1,
                chrono::Weekday::Tue => 2,
                chrono::Weekday::Wed => 3,
                chrono::Weekday::Thu => 4,
                chrono::Weekday::Fri => 5,
                chrono::Weekday::Sat => 6,
            };

            let time_matches = sched.hour == current_hour && sched.minute == current_minute;
            let day_matches = sched.days.contains(&weekday_js);

            if time_matches && day_matches {
                // Avoid firing more than once per minute
                let key = (current_hour, current_minute);
                if last_fired_minute == Some(key) {
                    continue;
                }
                last_fired_minute = Some(key);

                println!(
                    "Scheduler: schedule matched ({:02}:{:02}, day {}). Starting sync...",
                    current_hour, current_minute, weekday_js
                );

                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::sync::run_sync_process(app_clone).await;
                });
            } else {
                // Reset so the same minute can fire again next day
                if last_fired_minute == Some((sched.hour, sched.minute)) {
                    if current_hour != sched.hour || current_minute != sched.minute {
                        last_fired_minute = None;
                    }
                }
            }
        }
    });
}
