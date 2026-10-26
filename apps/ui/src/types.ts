export type Stance = 'reports' | 'endorses' | 'debunks' | 'mentions';

export type ClaimKind =
  | 'fact'
  | 'causal'
  | 'concealment'
  | 'conspiracy'
  | 'prophecy'
  | 'other';

export type EntityKind =
  | 'public_actor'
  | 'organization'
  | 'location'
  | 'event'
  | 'document'
  | 'technology'
  | 'law'
  | 'work'
  | 'other';

export interface StanceCount {
  stance: Stance;
  count: number;
}

export interface ClaimSummary {
  id: string;
  text: string;
  kind: ClaimKind;
  lang: string | null;
  status: string;
  document_count: number;
  evidence_count: number;
  first_seen: string;
  last_seen: string;
  stances: StanceCount[];
}

export interface EvidenceItem {
  document_id: string;
  canonical_url: string;
  title: string;
  domain: string;
  stance: Stance;
  strength: number;
  quote: string;
  quote_hash: string;
  span_start: number | null;
  span_end: number | null;
  model_version: string;
  prompt_version: string;
  created_at: string;
}

export interface EntityItem {
  id: string;
  kind: EntityKind;
  name: string;
  normalized: string;
}

export interface ClaimDetail {
  claim: ClaimSummary;
  evidence: EvidenceItem[];
  entities: EntityItem[];
}

export interface LedgerStats {
  claims: number;
  evidence: number;
  entities: number;
  documents_with_evidence: number;
}

export interface ClaimsResponse {
  claims: ClaimSummary[];
  stats: LedgerStats;
  generated_at: string;
}

export interface SearxngSettings {
  enabled: boolean;
  baseUrl: string;
  language: string | null;
  safesearch: number;
}

export interface BraveSettings {
  enabled: boolean;
  country: string | null;
  apiKeyConfigured: boolean;
  apiKeySource: string;
}

export interface LlmSettingsView {
  provider: string;
  fallbackProvider: string;
  baseUrl: string;
  model: string;
  embeddedModel: string;
  embeddedInstalled: boolean;
  apiKeyEnv: string;
  apiKeyConfigured: boolean;
  apiKeySource: string;
  fallbackBaseUrl: string;
  fallbackModel: string;
  fallbackApiKeyEnv: string;
  fallbackApiKeyConfigured: boolean;
  fallbackApiKeySource: string;
  timeoutSecs: number;
  connectTimeoutSecs: number;
  temperature: number;
  maxTokens: number;
  maxInputChars: number;
  maxClaims: number;
  maxAttempts: number;
}

export interface ScoutSettingsView {
  enabled: boolean;
  intervalDays: number;
  lastRun: string | null;
  inTrial: boolean;
  trialModel: string | null;
  previousModel: string | null;
}

export interface SettingsView {
  configPath: string;
  language: string;
  searxng: SearxngSettings;
  brave: BraveSettings;
  llm: LlmSettingsView;
  scout: ScoutSettingsView;
  envOverrides: string[];
}

export interface SettingsUpdate {
  language?: string;
  searxngEnabled?: boolean;
  searxngBaseUrl?: string;
  searxngLanguage?: string;
  searxngSafesearch?: number;
  braveEnabled?: boolean;
  braveCountry?: string;
  braveApiKey?: string;
  clearBraveApiKey?: boolean;
  llmProvider?: string;
  llmBaseUrl?: string;
  llmModel?: string;
  embeddedModel?: string;
  llmApiKeyEnv?: string;
  llmApiKey?: string;
  clearLlmApiKey?: boolean;
  fallbackProvider?: string;
  fallbackBaseUrl?: string;
  fallbackModel?: string;
  fallbackApiKeyEnv?: string;
  fallbackApiKey?: string;
  clearFallbackApiKey?: boolean;
  timeoutSecs?: number;
  connectTimeoutSecs?: number;
  temperature?: number;
  maxTokens?: number;
  maxInputChars?: number;
  maxClaims?: number;
  maxAttempts?: number;
  scoutEnabled?: boolean;
  scoutIntervalDays?: number;
}

export interface DiscoveredEngine {
  id: string;
  name: string;
  providerType: string;
  baseUrl: string;
  available: boolean;
  models: string[];
}

export interface DiscoveryResponse {
  engines: DiscoveredEngine[];
}

export interface EmbeddedModelStatusItem {
  id: string;
  name: string;
  description: string;
  size_bytes: number;
  installed: boolean;
  is_current: boolean;
}

export interface EmbeddedModelsResponse {
  active_provider: string;
  current_model_id: string;
  models: EmbeddedModelStatusItem[];
}

export interface AcceptedClaimInfo {
  claim_id: string;
  text: string;
  kind: string;
  stance: string;
  quote: string;
  quote_hash: string;
  confidence: number;
}

export interface IngestResponse {
  url: string;
  title: string;
  domain: string;
  claims_found: number;
  claims_accepted: number;
  claims_rejected: number;
  accepted: AcceptedClaimInfo[];
  generated_at: string;
}

export interface CredibilityScore {
  score: number;
  tier: string;
  tier_label: string;
  badge: string;
  badge_color: string;
  domain_authority: number;
  factual_density: number;
  corroboration_count: number;
  reason: string;
  factors: string[];
}

export interface AnswerCredibility {
  overall_score: number;
  verdict: string;
  verdict_detail: string;
  badge_color: string;
  sources_evaluated: number;
  primary_sources_count: number;
  corroboration_level: string;
  key_strengths: string[];
}

export interface CandidateModel {
  id: string;
  name: string;
  architecture: string;
  param_size: string;
  size_mb: number;
  quality_score: number;
  release_date: string;
  highlights: string[];
  summary: string;
  installed: boolean;
  is_current: boolean;
  is_trial: boolean;
}

export interface ScoutReport {
  enabled: boolean;
  interval_days: number;
  last_checked: string;
  in_trial: boolean;
  active_model: string;
  trial_model: string | null;
  previous_model: string | null;
  candidates: CandidateModel[];
  new_models_count: number;
}

export interface TrialActionResponse {
  success: boolean;
  in_trial: boolean;
  current_model: string;
  message: string;
}

export interface ChatSource {
  title: string;
  url: string;
  domain: string;
  snippet: string;
  published?: string;
}

export interface ChatMessageItem {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  isUrl?: boolean;
  guardRefused?: boolean;
  ingest?: IngestResponse;
  matchingClaims?: ClaimSummary[];
  sources?: ChatSource[];
  autoResearched?: boolean;
  newClaimsCount?: number;
  followUps?: string[];
  credibility?: AnswerCredibility;
  inTrial?: boolean;
  trialModel?: string | null;
  previousModel?: string | null;
  createdAt: string;
}

export interface ChatRequest {
  message: string;
  history?: Array<{ role: string; content: string }>;
}

export interface ChatResponse {
  role: string;
  content: string;
  is_url: boolean;
  guard_refused: boolean;
  ingest?: IngestResponse;
  matching_claims: ClaimSummary[];
  sources?: ChatSource[];
  auto_researched?: boolean;
  new_claims_count?: number;
  follow_ups?: string[];
  credibility?: AnswerCredibility;
  in_trial?: boolean;
  trial_model?: string | null;
  previous_model?: string | null;
  insufficient_credits?: boolean;
  wallet?: WalletInfo;
}

export interface WalletInfo {
  balance: number;
  queries_made: number;
  query_cost: number;
  formatted: string;
}

export interface SearchHit {
  title: string;
  url: string;
  domain: string;
  snippet: string;
  score: number;
  providers: string[];
  published?: string;
  credibility?: CredibilityScore;
}

export interface SearchWebResponse {
  query: string;
  hits: SearchHit[];
  guard_refused: boolean;
  guard_message?: string;
  total: number;
  latency_ms: number;
}

export interface ResearchBot {
  id: string;
  name: string;
  query: string;
  interval_mins: number;
  enabled: boolean;
  last_run_at: string | null;
  next_run_at: string | null;
  run_count: number;
  claims_found: number;
  created_at: string;
  max_sources_per_run: number;
  auto_ingest: boolean;
}

export interface NewResearchBot {
  name: string;
  query: string;
  interval_mins: number;
  max_sources_per_run?: number;
  auto_ingest?: boolean;
}

export interface BotRunRecord {
  id: string;
  bot_id: string;
  started_at: string;
  finished_at: string;
  urls_checked: number;
  claims_added: number;
  error_msg?: string | null;
  status: string;
}

