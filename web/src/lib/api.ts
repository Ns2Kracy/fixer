export const API_BASE = "/api/v1";
export const API_SCHEMA_VERSION = 1 as const;

export type SchemaVersion = typeof API_SCHEMA_VERSION;

function isStringValue<Values extends readonly string[]>(
  value: string,
  values: Values,
): value is Values[number] {
  return values.some((candidate) => candidate === value);
}

const MEDIA_KINDS = ["anime", "book", "movie", "music", "television"] as const;
export type MediaKind = (typeof MEDIA_KINDS)[number];
export function isMediaKind(value: string): value is MediaKind {
  return isStringValue(value, MEDIA_KINDS);
}

const JOB_STATES = [
  "queued",
  "scanning",
  "searching",
  "resolving",
  "awaiting_confirmation",
  "planning",
  "writing",
  "completed",
  "failed",
  "cancelled",
  "interrupted",
] as const;
export type JobState = (typeof JOB_STATES)[number];
export function isJobState(value: string): value is JobState {
  return isStringValue(value, JOB_STATES);
}

export interface ApiErrorDto {
  code: string;
  message: string;
  details?: Record<string, string>;
  request_id: string;
}

export interface ErrorEnvelope {
  error: ApiErrorDto;
}

export interface HealthDto {
  schema_version: SchemaVersion;
  status: "ok";
  version: string;
}

export interface ProviderDto {
  id: string;
  name: string;
  media_kinds: MediaKind[];
  network: boolean;
  optional: boolean;
}

export interface ProvidersDto {
  schema_version: SchemaVersion;
  providers: ProviderDto[];
}

const PROVIDER_IDS = [
  "local",
  "tmdb",
  "bangumi",
  "anilist",
  "musicbrainz",
  "openlibrary",
] as const;
export type ProviderId = (typeof PROVIDER_IDS)[number];
export function isProviderId(value: string): value is ProviderId {
  return isStringValue(value, PROVIDER_IDS);
}

const OUTPUT_PRESETS = ["full", "metadata"] as const;
export type OutputPreset = (typeof OUTPUT_PRESETS)[number];
export function isOutputPreset(value: string): value is OutputPreset {
  return isStringValue(value, OUTPUT_PRESETS);
}

const CONFLICT_POLICIES = ["prefer_first", "review", "error"] as const;
export type ConflictPolicy = (typeof CONFLICT_POLICIES)[number];
export function isConflictPolicy(value: string): value is ConflictPolicy {
  return isStringValue(value, CONFLICT_POLICIES);
}

export interface ProviderEndpoints {
  tmdb: string;
  bangumi: string;
  anilist: string;
  musicbrainz: string;
  openlibrary: string;
  openlibrary_cover: string;
}

export interface WorkspaceSettingsBase {
  offline: boolean;
  proxy: string | null;
  preferred_locales: string[];
  timeout_seconds: number;
  output_preset: OutputPreset;
  conflict_policy: ConflictPolicy;
  enabled_providers: ProviderId[];
  provider_endpoints: ProviderEndpoints;
}

export interface SecretStatus {
  tmdb_api_token_configured: boolean;
  anilist_access_token_configured: boolean;
  tmdb_api_token_env?: string;
  anilist_access_token_env?: string;
}

export interface WorkspaceSettings extends WorkspaceSettingsBase {
  secrets: SecretStatus;
}

export interface UpdateWorkspaceSettingsRequest extends WorkspaceSettingsBase {
  tmdb_api_token: string | null;
  anilist_access_token: string | null;
  clear_tmdb_api_token: boolean;
  clear_anilist_access_token: boolean;
}

export interface SettingsEnvelope {
  schema_version: SchemaVersion;
  settings: WorkspaceSettings;
}

export interface RootSummary {
  id: string;
  label: string;
}

export interface RootsEnvelope {
  schema_version: SchemaVersion;
  roots: RootSummary[];
}

export interface LibraryEntry {
  name: string;
  path: string;
  kind: "directory" | "file";
  size_bytes?: number;
}

export interface LibraryEnvelope {
  schema_version: SchemaVersion;
  root_id: string;
  path: string;
  entries: LibraryEntry[];
  truncated: boolean;
}

export interface ListLibraryRequest {
  rootId: string;
  path?: string;
}

export interface DirectoryRef {
  root_id: string;
  path: string;
}

export type IngestionMediaMode = "auto" | { fixed: MediaKind };
export type IngestionPlacement =
  | "move"
  | "copy"
  | "hardlink"
  | "symlink"
  | "reflink";
export type IngestionRuleStatus =
  | "watching"
  | "processing"
  | "needs_review"
  | "paused"
  | "error";

export interface IngestionRuleRequest {
  name: string;
  source: DirectoryRef;
  destination: DirectoryRef;
  media_kind_mode: IngestionMediaMode;
  placement: IngestionPlacement;
  path_template_override: string | null;
  enabled: boolean;
}

export interface IngestionRuleDto extends IngestionRuleRequest {
  id: number;
  status: IngestionRuleStatus;
  review_count: number;
  last_error: string | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface IngestionRuleEnvelope {
  schema_version: SchemaVersion;
  rule: IngestionRuleDto;
}

export interface IngestionRuleListEnvelope {
  schema_version: SchemaVersion;
  rules: IngestionRuleDto[];
}

export interface IngestionScanEnvelope {
  schema_version: SchemaVersion;
  rule_id: number;
  requested: boolean;
}

export interface IngestionSourceReview {
  source_id: number;
  relative_path: string;
  media_kinds: MediaKind[];
}

export interface IngestionSourceReviewListEnvelope {
  schema_version: SchemaVersion;
  rule_id: number;
  reviews: IngestionSourceReview[];
}

export interface IngestionSourceResolveEnvelope {
  schema_version: SchemaVersion;
  source_id: number;
  job_id: number;
}

export interface SearchMatch {
  root_id: string;
  path: string;
  name: string;
}

export interface SearchRequest {
  mediaKind: MediaKind;
  query: string;
  limit?: number;
}

export interface SearchEnvelope {
  schema_version: SchemaVersion;
  media_kind: MediaKind;
  results: SearchMatch[];
  truncated: boolean;
}

export interface ProviderProbeEnvelope {
  schema_version: SchemaVersion;
  provider: ProviderId;
  ok: boolean;
  category: string;
  message: string;
}

export interface TemplateSample {
  title: string;
  id: string;
  year: number | null;
  edition: string | null;
}

export interface TemplatePreviewRequest {
  path_template: string;
  content_template: string;
  sample: TemplateSample;
}

export interface TemplatePreviewEnvelope {
  schema_version: SchemaVersion;
  path: string;
  content: string;
  content_bytes: number;
}

export interface AuthStatusResponse {
  schema_version: SchemaVersion;
  registration_required: boolean;
  authenticated: boolean;
  username: string | null;
}

export interface CredentialsRequest {
  username: string;
  password: string;
}

export interface SessionResponse {
  schema_version: SchemaVersion;
  username: string;
  csrf_token: string;
  expires_at_ms: number;
}

export interface CreateDirectoryJobRequest {
  media_kind: MediaKind;
  source: DirectoryRef;
  destination: DirectoryRef;
  placement: IngestionPlacement;
  apply: boolean;
}

export interface CreateRuleJobRequest {
  rule: "matching";
  source: DirectoryRef;
  media_kind?: MediaKind;
}

export interface CreatePathJobRequest {
  media_kind: MediaKind;
  input_path: string;
  apply: boolean;
}

export type CreateJobRequest =
  | CreateDirectoryJobRequest
  | CreateRuleJobRequest
  | CreatePathJobRequest;

export interface JobInputDto extends CreatePathJobRequest {
  schema_version: SchemaVersion;
  organization?: {
    destination_path: string;
    placement: IngestionPlacement;
    origin_rule_id?: number;
    auto_execute: boolean;
    path_template?: string;
  };
}

export interface ProgressSummary {
  schema_version: SchemaVersion;
  stage: string;
  completed_items: number;
  total_items: number | null;
}

export interface ReviewSummary {
  schema_version: SchemaVersion;
  candidate_count: number;
  conflict_count: number;
  automation_reason?:
    | "manual_job"
    | "candidate_list_truncated"
    | "no_candidates"
    | "metadata_conflicts"
    | "provider_enrichment_failed"
    | "diagnostics_truncated"
    | "invalid_plan"
    | "destination_collision";
}

export interface ReviewDecisionDto {
  schema_version: SchemaVersion;
  candidate_index: number;
  accepted_conflict_indexes: number[];
}

export interface PlanSummary {
  schema_version: SchemaVersion;
  operation_count: number;
  requires_confirmation: boolean;
  fingerprint?: string;
}

export interface ExecutionFailureSummary {
  schema_version: SchemaVersion;
  operation_index?: number;
  code: string;
  message: string;
  phase?: string;
}

export interface ProviderTarget {
  media_kind: MediaKind;
  provider: string;
  external_id: ExternalIdArtifact;
}

export type OperationOutcome = "dry_run" | "succeeded" | "failed";

export interface OperationReport {
  operation_index: number;
  kind:
    | "create_directory"
    | "write_bytes"
    | "copy"
    | "move"
    | "symlink"
    | "hardlink"
    | "reflink";
  source?: string;
  destination: string;
  outcome: OperationOutcome;
  fingerprint?: string;
}

export interface ExecutionSummary {
  schema_version: SchemaVersion;
  completed_operations: number;
  failed_operations: number;
  failure?: ExecutionFailureSummary;
  operations?: OperationReport[];
}

export type ScrapeRunStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface ScrapeRunDto {
  id: number;
  item_name: string;
  media_kind: MediaKind;
  status: ScrapeRunStatus;
  requested_target?: ProviderTarget;
  selected_target?: ProviderTarget;
  correction_of?: number;
  retry_of?: number;
  candidate_count: number;
  conflict_count: number;
  execution?: ExecutionSummary;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface ScrapeRunEnvelope {
  schema_version: SchemaVersion;
  run: ScrapeRunDto;
}

export interface ScrapeRunListEnvelope {
  schema_version: SchemaVersion;
  runs: ScrapeRunDto[];
  has_more: boolean;
}

export interface CreateScrapeRunRequest {
  media_kind?: MediaKind;
  input_path?: string;
  source?: DirectoryRef;
  target?: ProviderTarget;
  correction_of?: number;
}

export interface JobDto {
  id: number;
  input: JobInputDto;
  state: JobState;
  progress?: ProgressSummary;
  review?: ReviewSummary;
  review_decision?: ReviewDecisionDto;
  plan?: PlanSummary;
  execution?: ExecutionSummary;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface JobEnvelope {
  schema_version: SchemaVersion;
  job: JobDto;
}

export interface JobListEnvelope {
  schema_version: SchemaVersion;
  jobs: JobDto[];
  has_more: boolean;
}

export interface ListJobsRequest {
  limit?: number;
  state?: JobState;
}

export interface ExternalIdArtifact {
  namespace: string;
  value: string;
}

export interface CandidateArtifact {
  index: number;
  media_kind: MediaKind;
  provider: string;
  external_id: ExternalIdArtifact;
  title: string;
  year?: number;
  sequence?: string;
}

export interface WarningArtifact {
  code: string;
  message: string;
}

export interface SourceArtifact {
  provider: string;
  external_id?: ExternalIdArtifact;
  locale?: string;
}

export interface ConflictArtifact {
  index: number;
  field_path: string;
  message: string;
  providers: string[];
  providers_truncated: boolean;
  sources: SourceArtifact[];
  sources_truncated: boolean;
}

export interface ReviewArtifactsEnvelope {
  schema_version: SchemaVersion;
  job_id: number;
  selected_candidate_index: number;
  candidates: CandidateArtifact[];
  candidates_truncated: boolean;
  warnings: WarningArtifact[];
  warnings_truncated: boolean;
  conflicts: ConflictArtifact[];
  conflicts_truncated: boolean;
}

export type OutputOperationKind =
  | "create_directory"
  | "write"
  | "copy"
  | "move"
  | "symlink"
  | "hardlink"
  | "reflink";

export interface OperationArtifact {
  index: number;
  kind: OutputOperationKind;
  source: string | null;
  target: string;
  content_bytes?: number;
}

export interface PlanArtifactsEnvelope {
  schema_version: SchemaVersion;
  job_id: number;
  output_root: string;
  operations: OperationArtifact[];
  operations_truncated: boolean;
  requires_approval: boolean;
}

export interface ReviewJobRequest {
  candidate_index: number;
  accepted_conflict_indexes: number[];
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: Record<string, string> | undefined;
  readonly requestId: string | undefined;

  constructor(status: number, dto?: Partial<ApiErrorDto>) {
    super(dto?.message ?? `Request failed with status ${status}`);
    this.name = "ApiError";
    this.status = status;
    this.code = dto?.code ?? "unexpected_response";
    this.details = dto?.details;
    this.requestId = dto?.request_id;
  }
}

type Fetch = (
  input: RequestInfo | URL,
  init?: RequestInit,
) => Promise<Response>;

export interface ApiClientOptions {
  baseUrl?: string;
  fetch?: Fetch;
  csrfToken?: () => string | undefined;
  csrfTokenChanged?: (token: string | undefined) => void;
}

export class ApiClient {
  readonly #baseUrl: string;
  readonly #fetch: Fetch;
  readonly #csrfToken: () => string | undefined;
  readonly #csrfTokenChanged: (token: string | undefined) => void;
  #issuedCsrfToken: string | undefined;

  constructor(options: ApiClientOptions = {}) {
    this.#baseUrl = options.baseUrl ?? API_BASE;
    this.#fetch =
      options.fetch ?? ((input, init) => globalThis.fetch(input, init));
    this.#csrfToken = options.csrfToken ?? (() => {});
    this.#csrfTokenChanged = options.csrfTokenChanged ?? (() => {});
  }

  health(): Promise<HealthDto> {
    return this.#request("/health");
  }

  providers(): Promise<ProvidersDto> {
    return this.#request("/providers");
  }

  settings(): Promise<SettingsEnvelope> {
    return this.#request("/settings");
  }

  updateSettings(
    request: UpdateWorkspaceSettingsRequest,
  ): Promise<SettingsEnvelope> {
    return this.#request("/settings", { method: "PUT", body: request });
  }

  libraryRoots(): Promise<RootsEnvelope> {
    return this.#request("/library/roots");
  }

  listLibrary(request: ListLibraryRequest): Promise<LibraryEnvelope> {
    const query = new URLSearchParams({
      root_id: request.rootId,
      path: request.path ?? "",
    });
    return this.#request(`/library?${query.toString()}`);
  }

  listIngestionRules(): Promise<IngestionRuleListEnvelope> {
    return this.#request("/ingestion-rules");
  }

  createIngestionRule(
    request: IngestionRuleRequest,
  ): Promise<IngestionRuleEnvelope> {
    return this.#request("/ingestion-rules", { method: "POST", body: request });
  }

  updateIngestionRule(
    id: number,
    request: IngestionRuleRequest,
  ): Promise<IngestionRuleEnvelope> {
    return this.#request(`/ingestion-rules/${id}`, {
      method: "PUT",
      body: request,
    });
  }

  deleteIngestionRule(id: number): Promise<void> {
    return this.#request(`/ingestion-rules/${id}`, { method: "DELETE" });
  }

  rescanIngestionRule(id: number): Promise<IngestionScanEnvelope> {
    return this.#request(`/ingestion-rules/${id}/scan`, { method: "POST" });
  }

  listIngestionReviews(id: number): Promise<IngestionSourceReviewListEnvelope> {
    return this.#request(`/ingestion-rules/${id}/reviews`);
  }

  resolveIngestionReview(
    sourceId: number,
    mediaKind: MediaKind,
  ): Promise<IngestionSourceResolveEnvelope> {
    return this.#request(`/ingestion-sources/${sourceId}/resolve`, {
      method: "POST",
      body: { media_kind: mediaKind },
    });
  }

  search(request: SearchRequest): Promise<SearchEnvelope> {
    const query = new URLSearchParams({
      media_kind: request.mediaKind,
      query: request.query,
      limit: String(request.limit ?? 25),
    });
    return this.#request(`/search?${query.toString()}`);
  }

  testProvider(provider: ProviderId): Promise<ProviderProbeEnvelope> {
    return this.#request(`/providers/${encodeURIComponent(provider)}/test`, {
      method: "POST",
      body: {},
    });
  }

  previewTemplate(
    request: TemplatePreviewRequest,
  ): Promise<TemplatePreviewEnvelope> {
    return this.#request("/templates/preview", {
      method: "POST",
      body: request,
    });
  }

  authStatus(): Promise<AuthStatusResponse> {
    return this.#request("/auth/status");
  }

  async register(request: CredentialsRequest): Promise<SessionResponse> {
    const response = await this.#request<SessionResponse>("/auth/register", {
      method: "POST",
      body: request,
    });
    this.#issuedCsrfToken = response.csrf_token;
    this.#csrfTokenChanged(response.csrf_token);
    return response;
  }

  async login(request: CredentialsRequest): Promise<SessionResponse> {
    const response = await this.#request<SessionResponse>("/auth/login", {
      method: "POST",
      body: request,
    });
    this.#issuedCsrfToken = response.csrf_token;
    this.#csrfTokenChanged(response.csrf_token);
    return response;
  }

  async logout(): Promise<void> {
    await this.#request("/auth/logout", { method: "POST" });
    this.#issuedCsrfToken = undefined;
    this.#csrfTokenChanged(this.#issuedCsrfToken);
  }

  createScrapeRun(request: CreateScrapeRunRequest): Promise<ScrapeRunEnvelope> {
    return this.#request("/scrape-runs", { method: "POST", body: request });
  }

  getScrapeRun(id: number): Promise<ScrapeRunEnvelope> {
    return this.#request(`/scrape-runs/${id}`);
  }

  listScrapeRuns(limit = 50): Promise<ScrapeRunListEnvelope> {
    return this.#request(`/scrape-runs?limit=${limit}`);
  }

  retryScrapeRun(id: number): Promise<ScrapeRunEnvelope> {
    return this.#request(`/scrape-runs/${id}/retry`, { method: "POST" });
  }

  cancelScrapeRun(id: number): Promise<ScrapeRunEnvelope> {
    return this.#request(`/scrape-runs/${id}/cancel`, { method: "POST" });
  }

  createJob(request: CreateJobRequest): Promise<JobEnvelope> {
    return this.#request("/jobs", { method: "POST", body: request });
  }

  getJob(id: number): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}`);
  }

  listJobs(request: ListJobsRequest = {}): Promise<JobListEnvelope> {
    const query = new URLSearchParams();
    if (request.limit !== undefined) query.set("limit", String(request.limit));
    if (request.state !== undefined) query.set("state", request.state);
    const suffix = query.size === 0 ? "" : `?${query.toString()}`;
    return this.#request(`/jobs${suffix}`);
  }

  retryJob(id: number): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/retry`, { method: "POST" });
  }

  getJobReview(
    id: number,
    candidateIndex?: number,
  ): Promise<ReviewArtifactsEnvelope> {
    const suffix =
      candidateIndex === undefined ? "" : `?candidate_index=${candidateIndex}`;
    return this.#request(`/jobs/${id}/review${suffix}`);
  }

  getJobPlan(id: number): Promise<PlanArtifactsEnvelope> {
    return this.#request(`/jobs/${id}/plan`);
  }

  cancelJob(id: number): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/cancel`, { method: "POST" });
  }

  reviewJob(id: number, request: ReviewJobRequest): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/review`, {
      method: "POST",
      body: request,
    });
  }

  executeJob(id: number, idempotencyKey: string): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/execute`, {
      method: "POST",
      body: { approved: true },
      headers: { "idempotency-key": idempotencyKey },
    });
  }

  async #request<T>(
    path: string,
    options: {
      method?: "GET" | "POST" | "PUT" | "DELETE";
      body?: unknown;
      headers?: Record<string, string>;
    } = {},
  ): Promise<T> {
    const method = options.method ?? "GET";
    const headers: Record<string, string> = { ...options.headers };
    if (options.body !== undefined)
      headers["content-type"] = "application/json";
    if (method !== "GET") {
      const csrfToken = this.#csrfToken() ?? this.#issuedCsrfToken;
      if (csrfToken !== undefined && csrfToken !== "")
        headers["x-csrf-token"] = csrfToken;
    }

    const response = await this.#fetch(`${this.#baseUrl}${path}`, {
      method,
      credentials: "same-origin",
      headers,
      ...(options.body === undefined
        ? {}
        : { body: JSON.stringify(options.body) }),
    });

    if (!response.ok) {
      const body = await readJson<Partial<ErrorEnvelope>>(response);
      throw new ApiError(response.status, body?.error);
    }
    if (response.status === 204) return undefined as T;

    const body = await readJson<T>(response);
    if (body === undefined) throw new ApiError(response.status);
    return body;
  }
}

async function readJson<T>(response: Response): Promise<T | undefined> {
  const contentType = response.headers.get("content-type") ?? "";
  if (!contentType.includes("application/json")) return undefined;
  try {
    return (await response.json()) as T;
  } catch {
    return undefined;
  }
}

const CSRF_COOKIE_NAME = "fixer_csrf";
const CSRF_STORAGE_KEY = "fixer.csrf-token";

function csrfCookieToken(): string | undefined {
  try {
    const prefix = `${CSRF_COOKIE_NAME}=`;
    const cookie = globalThis.document?.cookie
      .split(";")
      .map((value) => value.trim())
      .find((value) => value.startsWith(prefix));
    const token = cookie?.slice(prefix.length);
    return token === "" ? undefined : token;
  } catch {
    return undefined;
  }
}

function sessionCsrfToken(): string | undefined {
  const cookie = csrfCookieToken();
  if (cookie !== undefined) return cookie;
  try {
    return globalThis.sessionStorage?.getItem(CSRF_STORAGE_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

function storeSessionCsrfToken(token: string | undefined): void {
  try {
    if (token !== undefined && token !== "")
      globalThis.sessionStorage?.setItem(CSRF_STORAGE_KEY, token);
    else globalThis.sessionStorage?.removeItem(CSRF_STORAGE_KEY);
  } catch {
    // Storage can be unavailable in hardened or non-browser runtimes.
  }
}

export const api = new ApiClient({
  csrfToken: sessionCsrfToken,
  csrfTokenChanged: storeSessionCsrfToken,
});
