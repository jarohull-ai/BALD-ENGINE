#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bse_api::{
    ChatRequest, ChatResponse, ClaimDetail, ClaimsResponse, DiscoveryResponse, IngestResponse,
    LedgerStats, Service, SettingsUpdate, SettingsView,
};

struct AppState {
    service: Service,
}

#[tauri::command]
async fn discover_models(
    state: tauri::State<'_, AppState>,
) -> Result<DiscoveryResponse, String> {
    Ok(state.service.discover_models().await)
}

#[tauri::command]
fn claims_list(
    state: tauri::State<'_, AppState>,
    limit: usize,
) -> Result<ClaimsResponse, String> {
    state.service.claims(limit).map_err(|error| error.to_string())
}

#[tauri::command]
fn claim_detail(
    state: tauri::State<'_, AppState>,
    claim_id: String,
) -> Result<ClaimDetail, String> {
    state
        .service
        .claim(&claim_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn ledger_stats(state: tauri::State<'_, AppState>) -> Result<LedgerStats, String> {
    state.service.stats().map_err(|error| error.to_string())
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<SettingsView, String> {
    state.service.settings().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_settings(
    state: tauri::State<'_, AppState>,
    update: SettingsUpdate,
) -> Result<SettingsView, String> {
    state
        .service
        .save_settings(update)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn chat(
    state: tauri::State<'_, AppState>,
    request: ChatRequest,
) -> Result<ChatResponse, String> {
    state
        .service
        .chat(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn ingest_evidence(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<IngestResponse, String> {
    state
        .service
        .ingest(&url)
        .await
        .map_err(|error| error.to_string())
}

fn main() {
    let service = Service::open().expect("cannot open BALD service");

    tauri::Builder::default()
        .manage(AppState { service })
        .invoke_handler(tauri::generate_handler![
            claims_list,
            claim_detail,
            ledger_stats,
            get_settings,
            save_settings,
            discover_models,
            chat,
            ingest_evidence
        ])
        .run(tauri::generate_context!())
        .expect("error while running BALD desktop");
}
