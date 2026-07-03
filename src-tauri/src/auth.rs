use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tauri::{AppHandle, Emitter, Manager};
use keyring::Entry;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REDIRECT_URI: &str = "http://localhost:8080";

fn get_client_id() -> String {
    option_env!("GOOGLE_CLIENT_ID")
        .unwrap_or("")
        .to_string()
}

fn get_client_secret() -> String {
    option_env!("GOOGLE_CLIENT_SECRET")
        .unwrap_or("")
        .to_string()
}

fn create_client() -> BasicClient {
    BasicClient::new(
        ClientId::new(get_client_id()),
        Some(ClientSecret::new(get_client_secret())),
        AuthUrl::new(AUTH_URL.to_string()).unwrap(),
        Some(TokenUrl::new(TOKEN_URL.to_string()).unwrap()),
    )
    .set_redirect_uri(RedirectUrl::new(REDIRECT_URI.to_string()).unwrap())
}

#[tauri::command]
pub async fn login_with_google(app: AppHandle) -> Result<String, String> {
    let client = create_client();

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("https://www.googleapis.com/auth/drive.file".to_string()))
        .add_scope(Scope::new("https://www.googleapis.com/auth/userinfo.profile".to_string()))
        .add_scope(Scope::new("https://www.googleapis.com/auth/userinfo.email".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Open browser
    let _ = std::process::Command::new("open")
        .arg(auth_url.as_str())
        .spawn()
        .or_else(|_| {
            std::process::Command::new("xdg-open")
                .arg(auth_url.as_str())
                .spawn()
        })
        .or_else(|_| {
            std::process::Command::new("cmd")
                .args(["/C", "start", auth_url.as_str()])
                .spawn()
        });

    // Start a temporary local server to catch the redirect
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .map_err(|e| format!("Failed to bind to 8080: {}", e))?;
    let mut auth_code = String::new();
    while let Ok((mut stream, _)) = listener.accept().await {
        let mut buffer = [0; 2048];
        let _ = stream.read(&mut buffer).await;
        let request = String::from_utf8_lossy(&buffer);

        let mut code_found = false;
        if let Some(get_pos) = request.find("GET ") {
            let path_start = get_pos + 4;
            if let Some(path_end) = request[path_start..].find(' ') {
                let full_path = &request[path_start..path_start + path_end];
                if let Some(query_pos) = full_path.find('?') {
                    let query = &full_path[query_pos + 1..];
                    let params = url::form_urlencoded::parse(query.as_bytes());
                    for (key, val) in params {
                        if key == "code" {
                            auth_code = val.into_owned();
                            code_found = true;
                        }
                    }
                }
            }
        }

        if code_found {
            let response = "HTTP/1.1 200 OK\r\n\r\n<html><body><h2>Login successful!</h2><p>You can close this tab and return to the application.</p><script>window.close();</script></body></html>";
            let _ = stream.write_all(response.as_bytes()).await;
            break;
        } else {
            let response = "HTTP/1.1 404 Not Found\r\n\r\n";
            let _ = stream.write_all(response.as_bytes()).await;
        }
    }
    if auth_code.is_empty() {
        return Err("Authorization code not found".to_string());
    }

    let token_result = client
        .exchange_code(AuthorizationCode::new(auth_code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await;

    match token_result {
        Ok(token) => {
            let access_token = token.access_token().secret().to_string();
            // Store token using fallback storage helpers
            let _ = store_token(&app, &access_token);
            Ok("Login successful".to_string())
        }
        Err(e) => Err(format!("Token exchange failed: {:?}", e)),
    }
}

fn get_token_fallback_path(app: &AppHandle) -> std::path::PathBuf {
    let mut path = app.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }
    path.push("access_token.dat");
    path
}

fn store_token(app: &AppHandle, token: &str) -> Result<(), String> {
    // Write to fallback file to ensure availability in dev mode
    let path = get_token_fallback_path(app);
    std::fs::write(&path, token).map_err(|e| format!("Failed to write fallback token: {}", e))?;
    println!("Stored token in fallback file: {:?}", path);

    // Also try keyring
    let entry_res = Entry::new("google_drive_desktop", "access_token");
    if let Ok(entry) = entry_res {
        let _ = entry.set_password(token);
    }
    
    // Emit auth-status-changed event
    let _ = app.emit("auth-status-changed", true);
    
    Ok(())
}

pub fn retrieve_token(app: &AppHandle) -> Result<String, String> {
    // Fallback to file first (always contains the freshest token written during login)
    let path = get_token_fallback_path(app);
    if path.exists() {
        if let Ok(token) = std::fs::read_to_string(&path) {
            let trimmed = token.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
    }
    // Try keyring next
    let entry_res = Entry::new("google_drive_desktop", "access_token");
    if let Ok(entry) = entry_res {
        if let Ok(password) = entry.get_password() {
            return Ok(password);
        }
    }
    Err("No token found".to_string())
}

pub fn delete_token(app: &AppHandle) -> Result<(), String> {
    // Try keyring first
    let entry_res = Entry::new("google_drive_desktop", "access_token");
    if let Ok(entry) = entry_res {
        let _ = entry.delete_credential();
    }
    // Fallback to file
    let path = get_token_fallback_path(app);
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    
    // Emit auth-status-changed event
    let _ = app.emit("auth-status-changed", false);
    
    Ok(())
}

#[tauri::command]
pub fn check_auth_status(app: AppHandle) -> Result<bool, String> {
    Ok(retrieve_token(&app).is_ok())
}

#[tauri::command]
pub fn logout(app: AppHandle) -> Result<(), String> {
    delete_token(&app)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UserProfile {
    pub name: String,
    pub email: String,
    pub picture: String,
}

#[tauri::command]
pub async fn get_user_profile(app: AppHandle) -> Result<UserProfile, String> {
    let access_token = retrieve_token(&app)?;

    let client = reqwest::Client::new();
    let mut retries = 3;
    let mut delay = std::time::Duration::from_millis(500);
    let mut res;

    loop {
        let req = client.get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(&access_token);
        res = req.send().await;

        match &res {
            Ok(response) if response.status().is_success() => break,
            Ok(response) if response.status().is_server_error() && retries > 0 => {
                println!("get_user_profile returned 5xx, retrying in {:?}...", delay);
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
        let profile = res.json::<UserProfile>().await.map_err(|e| e.to_string())?;
        Ok(profile)
    } else {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let _ = delete_token(&app);
        }
        Err(format!("Failed to fetch user profile: {}", status))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StorageQuota {
    pub limit: String,
    pub usage: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AboutResponse {
    #[serde(rename = "storageQuota")]
    pub storage_quota: StorageQuota,
}

#[tauri::command]
pub async fn get_storage_quota(app: AppHandle) -> Result<AboutResponse, String> {
    let access_token = retrieve_token(&app)?;

    let client = reqwest::Client::new();
    let mut retries = 3;
    let mut delay = std::time::Duration::from_millis(500);
    let mut res;

    loop {
        let req = client.get("https://www.googleapis.com/drive/v3/about?fields=storageQuota")
            .bearer_auth(&access_token);
        res = req.send().await;

        match &res {
            Ok(response) if response.status().is_success() => break,
            Ok(response) if response.status().is_server_error() && retries > 0 => {
                println!("get_storage_quota returned 5xx, retrying in {:?}...", delay);
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
        let about = res.json::<AboutResponse>().await.map_err(|e| e.to_string())?;
        Ok(about)
    } else {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let _ = delete_token(&app);
        }
        Err(format!("Failed to fetch storage quota: {}", status))
    }
}
