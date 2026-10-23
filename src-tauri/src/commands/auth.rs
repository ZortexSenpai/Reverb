use reverb_core::audio::EngineCommand;
use reverb_core::subsonic::models::ServerInfo;
use reverb_core::subsonic::Credentials;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::ReverbError;
use crate::library_index;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub configured: bool,
    pub server_url: Option<String>,
    pub username: Option<String>,
    /// True when credentials had to be stored in a plaintext file because no
    /// OS keychain was available.
    pub insecure_storage: bool,
}

#[tauri::command]
pub fn auth_get_status(state: State<'_, AppState>) -> AuthStatus {
    let settings = state.settings.get();
    let configured = state.subsonic.credentials().is_some();
    AuthStatus {
        configured,
        server_url: settings.server_url,
        username: settings.username,
        insecure_storage: state.keychain.using_fallback(),
    }
}

#[tauri::command]
pub async fn auth_test(
    server_url: String,
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<ServerInfo, ReverbError> {
    let creds = Credentials::new(&server_url, &username, &password)?;
    Ok(state.subsonic.ping_with(&creds).await?)
}

#[tauri::command]
pub async fn auth_save(
    server_url: String,
    username: String,
    password: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ServerInfo, ReverbError> {
    let creds = Credentials::new(&server_url, &username, &password)?;
    let info = state.subsonic.ping_with(&creds).await?;

    state.keychain.store(creds.server_url.as_str(), &username, &password)?;
    state.settings.update(|s| {
        s.server_url = Some(creds.server_url.to_string());
        s.username = Some(username.clone());
    })?;
    state.subsonic.set_credentials(Some(creds));
    // Another account needs its own Library index.
    tauri::async_runtime::spawn(library_index::warm_up(app));
    Ok(info)
}

#[tauri::command]
pub fn auth_sign_out(state: State<'_, AppState>) -> Result<(), ReverbError> {
    state.engine.send(EngineCommand::Stop);
    let settings = state.settings.get();
    if let (Some(url), Some(user)) = (&settings.server_url, &settings.username) {
        state.keychain.clear(url, user);
    }
    state.settings.update(|s| {
        s.server_url = None;
        s.username = None;
    })?;
    state.subsonic.set_credentials(None);
    Ok(())
}
