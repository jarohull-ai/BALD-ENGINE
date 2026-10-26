import type {
  BotRunRecord,
  ChatRequest,
  ChatResponse,
  ClaimDetail,
  ClaimsResponse,
  DiscoveryResponse,
  EmbeddedModelsResponse,
  IngestResponse,
  NewResearchBot,
  ResearchBot,
  ScoutReport,
  SearchWebResponse,
  SettingsUpdate,
  SettingsView,
  TrialActionResponse,
} from './types';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__);
}

function sessionToken(): string {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="bald-token"]');
  const token = meta?.content?.trim() ?? '';
  if (!token || token === '__BALD_TOKEN__') {
    throw new Error('Brak tokenu sesji UI. Uruchom `bald serve` albo odśwież stronę.');
  }
  return token;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const module = await import('@tauri-apps/api/core');
  return module.invoke<T>(command, args);
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set('x-bald-token', sessionToken());
  headers.set('accept', 'application/json');
  if (init.body !== undefined) {
    headers.set('content-type', 'application/json');
  }

  let response: Response;
  try {
    response = await fetch(path, { ...init, headers });
  } catch (error) {
    throw new Error(
      `Nie można połączyć się z lokalnym API BALD. Czy działa \`bald serve\`? (${String(error)})`,
    );
  }

  const text = await response.text();
  let payload: unknown = null;
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch {
      payload = text;
    }
  }

  if (!response.ok) {
    const message =
      payload && typeof payload === 'object' && 'error' in payload
        ? String((payload as { error: unknown }).error)
        : typeof payload === 'string' && payload
          ? payload
          : response.statusText;
    throw new Error(`API ${response.status}: ${message}`);
  }

  return payload as T;
}

export async function fetchClaims(limit = 100): Promise<ClaimsResponse> {
  const safeLimit = Math.min(Math.max(limit, 1), 500);
  if (isTauriRuntime()) {
    return invoke<ClaimsResponse>('claims_list', { limit: safeLimit });
  }
  return request<ClaimsResponse>(`/api/claims?limit=${safeLimit}`);
}

export async function fetchClaim(claimId: string): Promise<ClaimDetail> {
  if (isTauriRuntime()) {
    return invoke<ClaimDetail>('claim_detail', { claimId });
  }
  return request<ClaimDetail>(`/api/claims/${encodeURIComponent(claimId)}`);
}

export async function fetchSettings(): Promise<SettingsView> {
  if (isTauriRuntime()) {
    return invoke<SettingsView>('get_settings');
  }
  return request<SettingsView>('/api/settings');
}

export async function saveSettings(update: SettingsUpdate): Promise<SettingsView> {
  if (isTauriRuntime()) {
    return invoke<SettingsView>('save_settings', { update });
  }
  return request<SettingsView>('/api/settings', {
    method: 'PUT',
    body: JSON.stringify(update),
  });
}

export async function discoverModels(): Promise<DiscoveryResponse> {
  if (isTauriRuntime()) {
    return invoke<DiscoveryResponse>('discover_models');
  }
  return request<DiscoveryResponse>('/api/models/discover');
}

export async function fetchEmbeddedModels(): Promise<EmbeddedModelsResponse> {
  if (isTauriRuntime()) {
    return invoke<EmbeddedModelsResponse>('embedded_models_get');
  }
  return request<EmbeddedModelsResponse>('/api/models/embedded');
}

export async function downloadEmbeddedModel(modelId: string): Promise<{ success: boolean }> {
  if (isTauriRuntime()) {
    return invoke<{ success: boolean }>('embedded_models_download', { payload: { model_id: modelId } });
  }
  return request<{ success: boolean }>('/api/models/embedded/download', {
    method: 'POST',
    body: JSON.stringify({ model_id: modelId }),
  });
}

export async function sendChatMessage(requestPayload: ChatRequest): Promise<ChatResponse> {
  if (isTauriRuntime()) {
    return invoke<ChatResponse>('chat', { request: requestPayload });
  }
  return request<ChatResponse>('/api/chat', {
    method: 'POST',
    body: JSON.stringify(requestPayload),
  });
}

export async function ingestEvidence(url: string): Promise<IngestResponse> {
  if (isTauriRuntime()) {
    return invoke<IngestResponse>('ingest_evidence', { url });
  }
  return request<IngestResponse>('/api/evidence/ingest', {
    method: 'POST',
    body: JSON.stringify({ url }),
  });
}

export async function searchWeb(query: string, limit = 10): Promise<SearchWebResponse> {
  if (isTauriRuntime()) {
    return invoke<SearchWebResponse>('search_web', { query, limit });
  }
  return request<SearchWebResponse>(`/api/search?q=${encodeURIComponent(query)}&limit=${limit}`);
}

export async function fetchBots(): Promise<ResearchBot[]> {
  if (isTauriRuntime()) {
    return invoke<ResearchBot[]>('bots_list');
  }
  return request<ResearchBot[]>('/api/bots');
}

export async function createBot(requestPayload: NewResearchBot): Promise<ResearchBot> {
  if (isTauriRuntime()) {
    return invoke<ResearchBot>('bot_create', { request: requestPayload });
  }
  return request<ResearchBot>('/api/bots', {
    method: 'POST',
    body: JSON.stringify(requestPayload),
  });
}

export async function updateBot(bot: ResearchBot): Promise<void> {
  if (isTauriRuntime()) {
    return invoke<void>('bot_update', { bot });
  }
  await request<void>(`/api/bots/${encodeURIComponent(bot.id)}`, {
    method: 'PUT',
    body: JSON.stringify(bot),
  });
}

export async function deleteBot(id: string): Promise<boolean> {
  if (isTauriRuntime()) {
    return invoke<boolean>('bot_delete', { id });
  }
  const res = await request<{ deleted: boolean }>(`/api/bots/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  });
  return res.deleted;
}

export async function runBotNow(id: string): Promise<BotRunRecord> {
  if (isTauriRuntime()) {
    return invoke<BotRunRecord>('bot_run_now', { id });
  }
  return request<BotRunRecord>(`/api/bots/${encodeURIComponent(id)}/run`, {
    method: 'POST',
  });
}

export async function fetchBotRuns(id: string): Promise<BotRunRecord[]> {
  if (isTauriRuntime()) {
    return invoke<BotRunRecord[]>('bot_runs_list', { id, limit: 50 });
  }
  return request<BotRunRecord[]>(`/api/bots/${encodeURIComponent(id)}/runs`);
}

export async function fetchScoutReport(): Promise<ScoutReport> {
  if (isTauriRuntime()) {
    return invoke<ScoutReport>('scout_get');
  }
  return request<ScoutReport>('/api/models/scout');
}

export async function checkScoutModels(): Promise<ScoutReport> {
  if (isTauriRuntime()) {
    return invoke<ScoutReport>('scout_check');
  }
  return request<ScoutReport>('/api/models/scout/check', { method: 'POST' });
}

export async function startTrialModel(modelId: string): Promise<TrialActionResponse> {
  if (isTauriRuntime()) {
    return invoke<TrialActionResponse>('trial_start', { payload: { model_id: modelId } });
  }
  return request<TrialActionResponse>('/api/models/trial/start', {
    method: 'POST',
    body: JSON.stringify({ model_id: modelId }),
  });
}

export async function commitTrialModel(): Promise<TrialActionResponse> {
  if (isTauriRuntime()) {
    return invoke<TrialActionResponse>('trial_commit');
  }
  return request<TrialActionResponse>('/api/models/trial/commit', { method: 'POST' });
}

export async function rollbackTrialModel(): Promise<TrialActionResponse> {
  if (isTauriRuntime()) {
    return invoke<TrialActionResponse>('trial_rollback');
  }
  return request<TrialActionResponse>('/api/models/trial/rollback', { method: 'POST' });
}

