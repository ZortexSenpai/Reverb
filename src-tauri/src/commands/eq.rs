use reverb_core::audio::{EngineCommand, EqBand, EqState, PreampMode};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::ReverbError;
use crate::persistence::settings::EqPreset;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EqPresetMeta {
    pub id: String,
    pub name: String,
}

/// Apply + persist + broadcast. Every EQ mutation funnels through here so the
/// engine, settings.json, and all windows stay in sync.
fn commit(app: &AppHandle, state: &AppState, eq: EqState) -> Result<EqState, ReverbError> {
    state.engine.send(EngineCommand::SetEq(eq.clone()));
    state.settings.update(|s| s.eq = eq.clone())?;
    let _ = app.emit("eq:state", eq.clone());
    Ok(eq)
}

#[tauri::command]
pub fn eq_get_state(state: State<'_, AppState>) -> EqState {
    state.settings.get().eq
}

#[tauri::command]
pub fn eq_set_all(
    eq: EqState,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    commit(&app, &state, eq)?;
    Ok(())
}

#[tauri::command]
pub fn eq_set_band(
    index: usize,
    band: EqBand,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let mut eq = state.settings.get().eq;
    if index >= eq.bands.len() {
        return Err(ReverbError::State(format!("no EQ band at index {index}")));
    }
    eq.bands[index] = band;
    commit(&app, &state, eq)?;
    Ok(())
}

#[tauri::command]
pub fn eq_set_enabled(
    enabled: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let mut eq = state.settings.get().eq;
    eq.enabled = enabled;
    commit(&app, &state, eq)?;
    Ok(())
}

#[tauri::command]
pub fn eq_set_preamp(
    preamp: PreampMode,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    let mut eq = state.settings.get().eq;
    eq.preamp = preamp;
    commit(&app, &state, eq)?;
    Ok(())
}

#[tauri::command]
pub fn eq_list_presets(state: State<'_, AppState>) -> Vec<EqPresetMeta> {
    state
        .settings
        .get()
        .eq_presets
        .into_iter()
        .map(|p| EqPresetMeta { id: p.id, name: p.name })
        .collect()
}

#[tauri::command]
pub fn eq_save_preset(
    name: String,
    state: State<'_, AppState>,
) -> Result<EqPresetMeta, ReverbError> {
    let id = uuid::Uuid::new_v4().to_string();
    let preset = EqPreset { id: id.clone(), name: name.clone(), state: state.settings.get().eq };
    state.settings.update(|s| s.eq_presets.push(preset))?;
    Ok(EqPresetMeta { id, name })
}

#[tauri::command]
pub fn eq_load_preset(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<EqState, ReverbError> {
    let preset = state
        .settings
        .get()
        .eq_presets
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| ReverbError::State(format!("no preset {id}")))?;
    commit(&app, &state, preset.state)
}

#[tauri::command]
pub fn eq_delete_preset(id: String, state: State<'_, AppState>) -> Result<(), ReverbError> {
    state.settings.update(|s| s.eq_presets.retain(|p| p.id != id))?;
    Ok(())
}

#[tauri::command]
pub fn eq_rename_preset(
    id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), ReverbError> {
    state.settings.update(|s| {
        if let Some(p) = s.eq_presets.iter_mut().find(|p| p.id == id) {
            p.name = name;
        }
    })?;
    Ok(())
}
