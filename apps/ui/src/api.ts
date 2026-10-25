import type {
  ChatRequest,
  ChatResponse,
  ClaimDetail,
  ClaimsResponse,
  DiscoveryResponse,
  IngestResponse,
  SettingsUpdate,
  SettingsView,
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

