//! Microsoft OAuth device-code authentication and Minecraft session management.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MS_DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

// Public Azure Application Client ID for open-source launchers (Prism Launcher)
const CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";
const OAUTH_SCOPE: &str = "XboxLive.SignIn XboxLive.offline_access";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuthSession {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
}

#[derive(Deserialize, Debug)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
    message: Option<String>,
}

#[derive(Deserialize, Debug)]
struct MsTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    #[allow(dead_code)]
    expires_in: Option<u64>,
    error: Option<String>,
}

fn urlencode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn auth_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let data_home = xdg.get_data_home().ok_or("Failed to resolve XDG data home")?;
    fs::create_dir_all(&data_home)?;
    Ok(data_home.join("auth.json"))
}

pub fn load_session() -> Option<AuthSession> {
    let path = auth_file_path().ok()?;
    if !path.exists() {
        return None;
    }
    let file = File::open(&path).ok()?;
    serde_json::from_reader(file).ok()
}

pub fn save_session(session: &AuthSession) -> Result<(), Box<dyn std::error::Error>> {
    let path = auth_file_path()?;
    let json = serde_json::to_string_pretty(session)?;
    fs::write(path, json)?;
    Ok(())
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Initiates interactive Microsoft Device Flow login.
pub fn login() -> Result<AuthSession, Box<dyn std::error::Error>> {
    let client = Client::builder().tcp_nodelay(true).build()?;

    // Step 1: Request device authorization code
    let device_body = format!(
        "client_id={}&scope={}",
        urlencode(CLIENT_ID),
        urlencode(OAUTH_SCOPE)
    );
    let device_resp: DeviceCodeResponse = client
        .post(MS_DEVICE_CODE_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(device_body)
        .send()?
        .json()?;

    if let Some(msg) = &device_resp.message {
        println!("[nite] {msg}");
    } else {
        println!("[nite] To authenticate, open {} and enter code: {}", device_resp.verification_uri, device_resp.user_code);
    }
    println!("[nite] waiting for authorization in browser...");

    let interval = Duration::from_secs(device_resp.interval.unwrap_or(5).max(1));
    let deadline = SystemTime::now() + Duration::from_secs(device_resp.expires_in);

    // Step 2: Poll for MS OAuth token
    let ms_tokens: MsTokenResponse = loop {
        if SystemTime::now() > deadline {
            return Err("Authentication timed out. Please try again.".into());
        }

        thread::sleep(interval);

        let token_body = format!(
            "client_id={}&grant_type={}&device_code={}",
            urlencode(CLIENT_ID),
            urlencode("urn:ietf:params:oauth:grant-type:device_code"),
            urlencode(&device_resp.device_code)
        );

        let resp = client
            .post(MS_TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(token_body)
            .send()?;

        let body: MsTokenResponse = resp.json()?;
        if let Some(err) = &body.error {
            match err.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
                other => return Err(format!("Microsoft authorization error: {other}").into()),
            }
        }

        if body.access_token.is_some() {
            break body;
        }
    };

    let ms_access_token = ms_tokens.access_token.ok_or("Missing Microsoft access token")?;
    let refresh_token = ms_tokens.refresh_token;

    println!("[nite] authenticated with Microsoft, exchanging for Minecraft token...");

    // Step 3-7: Complete Xbox Live & Minecraft handshake
    let session = complete_mc_auth(&client, &ms_access_token, refresh_token)?;
    save_session(&session)?;

    println!("[nite] logged in as '{}' (UUID: {})", session.username, session.uuid);
    Ok(session)
}

/// Refreshes token if expired or nearly expired. Returns valid AuthSession.
pub fn get_valid_session() -> Option<AuthSession> {
    let session = load_session()?;
    let now = current_timestamp();

    // If session has more than 5 minutes remaining, it's valid
    if session.expires_at > now + 300 {
        return Some(session);
    }

    // Attempt refresh if refresh_token is present
    let refresh_tok = session.refresh_token.as_ref()?;
    let client = Client::builder().tcp_nodelay(true).build().ok()?;

    let refresh_body = format!(
        "client_id={}&grant_type=refresh_token&refresh_token={}",
        urlencode(CLIENT_ID),
        urlencode(refresh_tok)
    );

    let resp = client
        .post(MS_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(refresh_body)
        .send()
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let tokens: MsTokenResponse = resp.json().ok()?;
    let new_ms_token = tokens.access_token?;
    let new_refresh = tokens.refresh_token.or(session.refresh_token.clone());

    if let Ok(new_session) = complete_mc_auth(&client, &new_ms_token, new_refresh) {
        let _ = save_session(&new_session);
        Some(new_session)
    } else {
        None
    }
}

fn complete_mc_auth(
    client: &Client,
    ms_token: &str,
    refresh_token: Option<String>,
) -> Result<AuthSession, Box<dyn std::error::Error>> {
    // 3. Xbox Live User Authentication
    let ticket = if ms_token.starts_with("d=") {
        ms_token.to_string()
    } else {
        format!("d={ms_token}")
    };

    let xbl_payload = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": ticket
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let xbl_resp: serde_json::Value = client
        .post(XBL_AUTH_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("x-xbl-contract-version", "1")
        .json(&xbl_payload)
        .send()?
        .json()?;
    let xbl_token = xbl_resp["Token"]
        .as_str()
        .ok_or("Failed to obtain Xbox Live token")?;
    let user_hash = xbl_resp["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or("Failed to obtain Xbox Live user hash")?;

    // 4. XSTS Authentication
    let xsts_payload = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let xsts_resp: serde_json::Value = client.post(XSTS_AUTH_URL).json(&xsts_payload).send()?.json()?;

    if let Some(err_code) = xsts_resp.get("XErr").and_then(|v| v.as_u64()) {
        match err_code {
            2148916233 => return Err("This Microsoft account does not have an Xbox account.".into()),
            2148916238 => {
                return Err("Account is a child account and requires adult approval in Microsoft Family.".into())
            }
            _ => return Err(format!("XSTS authentication failed with error code: {err_code}").into()),
        }
    }

    let xsts_token = xsts_resp["Token"]
        .as_str()
        .ok_or("Failed to obtain XSTS token")?;

    // 5. Minecraft Authentication with Xbox
    let mc_auth_payload = serde_json::json!({
        "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}")
    });

    let mc_resp: serde_json::Value = client
        .post(MC_LOGIN_URL)
        .json(&mc_auth_payload)
        .send()?
        .json()?;

    let access_token = mc_resp["access_token"]
        .as_str()
        .ok_or("Failed to obtain Minecraft access token")?
        .to_string();
    let expires_in = mc_resp["expires_in"].as_u64().unwrap_or(86400);

    // 6. Minecraft Profile Query (UUID & username)
    let profile_resp: serde_json::Value = client
        .get(MC_PROFILE_URL)
        .bearer_auth(&access_token)
        .send()?
        .json()?;

    let username = profile_resp["name"]
        .as_str()
        .ok_or("Account does not own Minecraft Java Edition or has no Java profile")?
        .to_string();
    let uuid = profile_resp["id"]
        .as_str()
        .ok_or("Missing player UUID in Minecraft profile")?
        .to_string();

    let session = AuthSession {
        username,
        uuid,
        access_token,
        refresh_token,
        expires_at: current_timestamp() + expires_in,
    };

    Ok(session)
}
