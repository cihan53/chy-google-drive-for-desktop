use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

fn main() {
    // Look for .env in the workspace root or src-tauri
    let dotenv_paths = [
        Path::new("../.env"),
        Path::new(".env"),
    ];

    for path in &dotenv_paths {
        if path.exists() {
            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                for line in reader.lines().map_while(Result::ok) {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = trimmed.split_once('=') {
                        let key = key.trim();
                        let value = value.trim().trim_matches('"').trim_matches('\'');
                        println!("cargo:rustc-env={}={}", key, value);
                    }
                }
            }
            break;
        }
    }

    tauri_build::build()
}
