//! Lokalny serwer HTTP dla `bald serve`.
//!
//! Serwer binduje wyłącznie loopback. Token sesji jest losowy, wstrzykiwany do
//! `index.html` i wymagany w nagłówku `x-bald-token` dla każdego `/api/*`
//! poza `/api/health`. Brak CORS jest celowy: obca strona nie może odczytać
//! tokenu ani wysłać nagłówka bez preflightu.

use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context};
use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::discovery::{discover_local_engines, DiscoveryResponse};
use crate::service::{
    ChatRequest, ChatResponse, ClaimsResponse, IngestResponse, SearchWebResponse, Service,
    ServiceError,
};
use crate::settings::{SettingsError, SettingsUpdate, SettingsView};
use bse_store::{BotRunRecord, ClaimDetail, LedgerStats, NewResearchBot, ResearchBot};

static EMBEDDED_UI: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/ui_dist");

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub bind: SocketAddr,
    pub ui_dir: Option<PathBuf>,
    pub token: Option<String>,
}

#[derive(Clone)]
struct AppState {
    service: Arc<Service>,
    token: Arc<str>,
    ui_dir: Option<PathBuf>,
    ui_mode: &'static str,
}

#[derive(Debug, Deserialize)]
struct ClaimsQuery {
    limit: Option<usize>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.message,
                "status": self.status.as_u16(),
            })),
        )
            .into_response()
    }
}

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound(message) => ApiError::not_found(message),
            ServiceError::Settings(SettingsError::Invalid(message)) => {
                ApiError::bad_request(message)
            }
            ServiceError::Store(bse_store::StoreError::InvalidCode) => {
                ApiError::bad_request("Nieprawidłowy kod licencyjny.")
            }
            ServiceError::Store(bse_store::StoreError::CodeAlreadyRedeemed) => {
                ApiError::bad_request("Ten kod został już wcześniej wykorzystany.")
            }
            ServiceError::Store(bse_store::StoreError::InsufficientBalance(b)) => {
                ApiError::bad_request(format!("Niewystarczająca liczba kredytów (wymagane {b:.2} Bald$)"))
            }
            other => ApiError::internal(other),
        }
    }
}

pub async fn serve(options: ServeOptions) -> anyhow::Result<()> {
    if !options.bind.ip().is_loopback() {
        bail!(
            "bald serve może bindować wyłącznie loopback (otrzymano {})",
            options.bind.ip()
        );
    }

    let service = Arc::new(Service::open().context("cannot open BALD service")?);
    let token = options
        .token
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()));
    let ui_mode = if options.ui_dir.is_some() {
        "directory"
    } else {
        "embedded"
    };
    let state = AppState {
        service,
        token: Arc::from(token),
        ui_dir: options.ui_dir,
        ui_mode,
    };
    let app = router(state.clone());
    let listener = TcpListener::bind(options.bind)
        .await
        .with_context(|| format!("cannot bind {}", options.bind))?;
    let address = listener.local_addr().context("cannot read local address")?;

    println!("BALD UI · http://{address}/");
    println!("UI assets: {ui_mode}");
    println!("API: /api/health · /api/claims · /api/settings · /api/bots");
    println!("Naciśnij Ctrl+C, aby zatrzymać.");

    let bg_service = state.service.clone();
    tokio::spawn(async move {
        bot_scheduler_loop(bg_service).await;
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("local UI server failed")?;
    Ok(())
}

fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/claims", get(claims))
        .route("/claims/clear", axum::routing::post(claims_clear))
        .route("/claims/{id}", get(claim_detail).delete(claim_delete))
        .route("/stats", get(stats))
        .route("/wallet", get(wallet_get))
        .route("/wallet/redeem", axum::routing::post(wallet_redeem))
        .route("/settings", get(settings_get).put(settings_put))
        .route("/models/discover", get(models_discover))
        .route("/models/embedded", get(embedded_models_get))
        .route("/models/embedded/download", axum::routing::post(embedded_models_download))
        .route("/models/scout", get(scout_get))
        .route("/models/scout/check", axum::routing::post(scout_check))
        .route("/models/trial/start", axum::routing::post(trial_start))
        .route("/models/trial/commit", axum::routing::post(trial_commit))
        .route("/models/trial/rollback", axum::routing::post(trial_rollback))
        .route("/chat", axum::routing::post(chat_post))
        .route("/evidence/ingest", axum::routing::post(ingest_post))
        .route("/search", get(search_get).post(search_post))
        .route("/bots", get(bots_list).post(bots_create))
        .route(
            "/bots/{id}",
            get(bot_get).put(bot_update).delete(bot_delete),
        )
        .route("/bots/{id}/run", axum::routing::post(bot_run))
        .route("/bots/{id}/runs", get(bot_runs_list))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));

    Router::new()
        .route("/api/health", get(health))
        .nest("/api", protected)
        .fallback(static_asset)
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "product": "BALD",
        "version": env!("CARGO_PKG_VERSION"),
        "ui": state.ui_mode,
    }))
}

async fn claims(
    State(state): State<AppState>,
    Query(query): Query<ClaimsQuery>,
) -> Result<Json<ClaimsResponse>, ApiError> {
    let response = state
        .service
        .claims(query.limit.unwrap_or(100))
        .map_err(ApiError::from)?;
    Ok(Json(response))
}

async fn claim_detail(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ClaimDetail>, ApiError> {
    let detail = state.service.claim(&id).map_err(ApiError::from)?;
    Ok(Json(detail))
}

async fn claim_delete(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state.service.delete_claim(&id).map_err(ApiError::from)?;
    Ok(Json(json!({ "deleted": deleted, "id": id })))
}

async fn claims_clear(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let count = state.service.clear_claims().map_err(ApiError::from)?;
    Ok(Json(json!({ "cleared": count })))
}

async fn stats(State(state): State<AppState>) -> Result<Json<LedgerStats>, ApiError> {
    let stats = state.service.stats().map_err(ApiError::from)?;
    Ok(Json(stats))
}

async fn wallet_get(State(state): State<AppState>) -> Result<Json<bse_store::WalletInfo>, ApiError> {
    let wallet = state.service.wallet().map_err(ApiError::from)?;
    Ok(Json(wallet))
}

#[derive(Debug, Deserialize)]
struct RedeemRequest {
    code: String,
}

async fn wallet_redeem(
    State(state): State<AppState>,
    Json(req): Json<RedeemRequest>,
) -> Result<Json<bse_store::WalletInfo>, ApiError> {
    let wallet = state.service.redeem_voucher(&req.code).map_err(ApiError::from)?;
    Ok(Json(wallet))
}

async fn settings_get(State(state): State<AppState>) -> Result<Json<SettingsView>, ApiError> {
    let view = state.service.settings().map_err(ApiError::from)?;
    Ok(Json(view))
}

async fn settings_put(
    State(state): State<AppState>,
    Json(update): Json<SettingsUpdate>,
) -> Result<Json<SettingsView>, ApiError> {
    let view = state
        .service
        .save_settings(update)
        .map_err(ApiError::from)?;
    Ok(Json(view))
}

async fn models_discover() -> Json<DiscoveryResponse> {
    Json(discover_local_engines().await)
}

#[derive(Debug, Deserialize)]
struct DownloadModelPayload {
    model_id: String,
}

async fn embedded_models_get(
    State(state): State<AppState>,
) -> Result<Json<crate::service::EmbeddedModelsResponse>, ApiError> {
    state
        .service
        .list_embedded_models()
        .map(Json)
        .map_err(ApiError::from)
}

async fn embedded_models_download(
    State(state): State<AppState>,
    Json(payload): Json<DownloadModelPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let success = state
        .service
        .download_embedded_model(&payload.model_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "success": success })))
}

async fn scout_get(
    State(state): State<AppState>,
) -> Result<Json<crate::scout::ScoutReport>, ApiError> {
    state
        .service
        .scout_report()
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn scout_check(
    State(state): State<AppState>,
) -> Result<Json<crate::scout::ScoutReport>, ApiError> {
    state
        .service
        .scout_check()
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn trial_start(
    State(state): State<AppState>,
    Json(payload): Json<crate::scout::StartTrialRequest>,
) -> Result<Json<crate::scout::TrialActionResponse>, ApiError> {
    state
        .service
        .start_trial(&payload.model_id)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn trial_commit(
    State(state): State<AppState>,
) -> Result<Json<crate::scout::TrialActionResponse>, ApiError> {
    state
        .service
        .commit_trial()
        .map(Json)
        .map_err(ApiError::from)
}

async fn trial_rollback(
    State(state): State<AppState>,
) -> Result<Json<crate::scout::TrialActionResponse>, ApiError> {
    state
        .service
        .rollback_trial()
        .map(Json)
        .map_err(ApiError::from)
}

#[derive(Debug, Deserialize)]
struct IngestPayload {
    url: String,
}

async fn chat_post(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, ApiError> {
    state
        .service
        .chat(request)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn ingest_post(
    State(state): State<AppState>,
    Json(payload): Json<IngestPayload>,
) -> Result<Json<IngestResponse>, ApiError> {
    state
        .service
        .ingest(&payload.url)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

#[derive(Debug, Deserialize)]
struct SearchQueryParam {
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchPayload {
    query: String,
    limit: Option<usize>,
}

async fn search_get(
    State(state): State<AppState>,
    Query(query): Query<SearchQueryParam>,
) -> Result<Json<SearchWebResponse>, ApiError> {
    let q = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(10);
    state
        .service
        .search_web(&q, limit)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn search_post(
    State(state): State<AppState>,
    Json(payload): Json<SearchPayload>,
) -> Result<Json<SearchWebResponse>, ApiError> {
    let limit = payload.limit.unwrap_or(10);
    state
        .service
        .search_web(&payload.query, limit)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

async fn bots_list(State(state): State<AppState>) -> Result<Json<Vec<ResearchBot>>, ApiError> {
    let list = state.service.list_bots().map_err(ApiError::from)?;
    Ok(Json(list))
}

async fn bots_create(
    State(state): State<AppState>,
    Json(payload): Json<NewResearchBot>,
) -> Result<Json<ResearchBot>, ApiError> {
    let bot = state.service.create_bot(payload).map_err(ApiError::from)?;
    Ok(Json(bot))
}

async fn bot_get(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ResearchBot>, ApiError> {
    let bot = state.service.get_bot(&id).map_err(ApiError::from)?;
    Ok(Json(bot))
}

async fn bot_update(
    State(state): State<AppState>,
    AxumPath(_id): AxumPath<String>,
    Json(bot): Json<ResearchBot>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.service.update_bot(bot).map_err(ApiError::from)?;
    Ok(Json(json!({ "status": "updated" })))
}

async fn bot_delete(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state.service.delete_bot(&id).map_err(ApiError::from)?;
    Ok(Json(json!({ "deleted": deleted })))
}

async fn bot_run(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<BotRunRecord>, ApiError> {
    let run = state
        .service
        .execute_bot(&id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(run))
}

async fn bot_runs_list(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<BotRunRecord>>, ApiError> {
    let runs = state
        .service
        .list_bot_runs(&id, 50)
        .map_err(ApiError::from)?;
    Ok(Json(runs))
}

async fn bot_scheduler_loop(service: Arc<Service>) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        if let Ok(due_bots) = service.due_bots() {
            for bot in due_bots {
                println!(
                    "🤖 Bot Researcher: uruchamiam automatyczne badanie dla bota '{}' (id: {})",
                    bot.name, bot.id
                );
                match service.execute_bot(&bot.id).await {
                    Ok(run) => {
                        println!(
                            "🤖 Bot Researcher: zakończono przebieg '{}': status {}, zbadano {} URL, dodano {} twierdzeń",
                            bot.name, run.status, run.urls_checked, run.claims_added
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "🤖 Bot Researcher: błąd wykonania bota '{}': {}",
                            bot.name, e
                        );
                    }
                }
            }
        }
    }
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let provided = request
        .headers()
        .get("x-bald-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !constant_time_eq(provided.as_bytes(), state.token.as_bytes()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing or invalid session token" })),
        )
            .into_response();
    }
    next.run(request).await
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b;
    }
    diff == 0
}

async fn static_asset(State(state): State<AppState>, uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    if raw == "api" || raw.starts_with("api/") {
        return ApiError::not_found("unknown API route").into_response();
    }
    let requested = if raw.is_empty() { "index.html" } else { raw };
    let Some(relative) = safe_relative(requested) else {
        return ApiError::not_found("invalid asset path").into_response();
    };

    let mut served = relative.clone();
    let mut body = read_asset(&state, &served).await;
    if body.is_none() && served.extension().is_none() {
        served = PathBuf::from("index.html");
        body = read_asset(&state, &served).await;
    }
    let Some(bytes) = body else {
        return ApiError::not_found("asset not found").into_response();
    };

    let is_html = served == Path::new("index.html")
        || served.extension().and_then(|value| value.to_str()) == Some("html");
    let content_type = mime_for(&served);
    let cache = if is_html {
        "no-store"
    } else {
        "public, max-age=3600"
    };

    if is_html {
        let html = String::from_utf8_lossy(&bytes);
        let html = inject_token(&html, &state.token);
        let mut response = Html(html).into_response();
        set_static_headers(response.headers_mut(), content_type, cache, true);
        return response;
    }

    let mut response = ([(header::CONTENT_TYPE, content_type)], bytes).into_response();
    set_static_headers(response.headers_mut(), content_type, cache, false);
    response
}

fn set_static_headers(
    headers: &mut axum::http::HeaderMap,
    content_type: &'static str,
    cache: &'static str,
    html: bool,
) {
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    if html {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; style-src 'self' 'unsafe-inline'; \
                 script-src 'self'; img-src 'self' data:; font-src 'self' data:; \
                 connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; \
                 form-action 'self'",
            ),
        );
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    }
}

fn inject_token(html: &str, token: &str) -> String {
    let replaced = html.replace("__BALD_TOKEN__", token);
    if replaced != html {
        return replaced;
    }
    replaced.replace(
        "<meta name=\"bald-token\" content=\"\" />",
        &format!("<meta name=\"bald-token\" content=\"{token}\" />"),
    )
}

fn safe_relative(raw: &str) -> Option<PathBuf> {
    let path = Path::new(raw);
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if out.as_os_str().is_empty() {
        out.push("index.html");
    }
    Some(out)
}

async fn read_asset(state: &AppState, relative: &Path) -> Option<Vec<u8>> {
    if let Some(dir) = &state.ui_dir {
        return tokio::fs::read(dir.join(relative)).await.ok();
    }
    EMBEDDED_UI
        .get_file(relative)
        .map(|file| file.contents().to_vec())
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("map") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_comparison_is_exact() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn token_is_injected_into_both_build_shapes() {
        assert_eq!(
            inject_token("<meta content=\"__BALD_TOKEN__\">", "secret"),
            "<meta content=\"secret\">"
        );
        assert_eq!(
            inject_token("<meta name=\"bald-token\" content=\"\" />", "secret"),
            "<meta name=\"bald-token\" content=\"secret\" />"
        );
    }

    #[test]
    fn traversal_is_rejected() {
        assert!(safe_relative("../config.toml").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert_eq!(
            safe_relative("assets/app.js").expect("valid"),
            PathBuf::from("assets/app.js")
        );
    }
}
