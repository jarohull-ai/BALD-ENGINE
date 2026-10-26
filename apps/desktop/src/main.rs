#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bse_api::{
    BotRunRecord, ChatRequest, ChatResponse, ClaimDetail, ClaimsResponse, DiscoveryResponse,
    IngestResponse, LedgerStats, NewResearchBot, ResearchBot, SearchWebResponse, Service,
    SettingsUpdate, SettingsView, WalletInfo,
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

#[tauri::command]
async fn search_web(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<SearchWebResponse, String> {
    state
        .service
        .search_web(&query, limit)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn bots_list(state: tauri::State<'_, AppState>) -> Result<Vec<ResearchBot>, String> {
    state.service.list_bots().map_err(|e| e.to_string())
}

#[tauri::command]
fn bot_create(
    state: tauri::State<'_, AppState>,
    request: NewResearchBot,
) -> Result<ResearchBot, String> {
    state.service.create_bot(request).map_err(|e| e.to_string())
}

#[tauri::command]
fn bot_update(state: tauri::State<'_, AppState>, bot: ResearchBot) -> Result<(), String> {
    state.service.update_bot(bot).map_err(|e| e.to_string())
}

#[tauri::command]
fn bot_delete(state: tauri::State<'_, AppState>, id: String) -> Result<bool, String> {
    state.service.delete_bot(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn bot_run_now(state: tauri::State<'_, AppState>, id: String) -> Result<BotRunRecord, String> {
    state
        .service
        .execute_bot(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn bot_runs_list(
    state: tauri::State<'_, AppState>,
    id: String,
    limit: usize,
) -> Result<Vec<BotRunRecord>, String> {
    state.service.list_bot_runs(&id, limit).map_err(|e| e.to_string())
}

#[tauri::command]
fn claim_delete(state: tauri::State<'_, AppState>, claim_id: String) -> Result<bool, String> {
    state.service.delete_claim(&claim_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn claims_clear(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    state.service.clear_claims().map_err(|e| e.to_string())
}

#[tauri::command]
fn wallet_get(state: tauri::State<'_, AppState>) -> Result<WalletInfo, String> {
    state.service.wallet().map_err(|e| e.to_string())
}

#[tauri::command]
fn wallet_redeem(state: tauri::State<'_, AppState>, code: String) -> Result<WalletInfo, String> {
    state.service.redeem_voucher(&code).map_err(|e| e.to_string())
}

fn main() {
    let service = Service::open().expect("cannot open BALD service");

    tauri::Builder::default()
        .manage(AppState { service })
        .invoke_handler(tauri::generate_handler![
            claims_list,
            claim_detail,
            claim_delete,
            claims_clear,
            ledger_stats,
            get_settings,
            save_settings,
            discover_models,
            chat,
            ingest_evidence,
            search_web,
            bots_list,
            bot_create,
            bot_update,
            bot_delete,
            bot_run_now,
            bot_runs_list,
            wallet_get,
            wallet_redeem
        ])
        .run(tauri::generate_context!())
        .expect("error while running BALD desktop");
}
