export const API_BASE = '/api/v1'
export const API_SCHEMA_VERSION = 1 as const

export type SchemaVersion = typeof API_SCHEMA_VERSION
export type MediaKind = 'anime' | 'book' | 'movie' | 'music' | 'television'
export type JobState =
  | 'queued'
  | 'scanning'
  | 'searching'
  | 'resolving'
  | 'awaiting_confirmation'
  | 'planning'
  | 'writing'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'interrupted'

export interface ApiErrorDto {
  code: string
  message: string
  details?: Record<string, string>
  request_id: string
}

export interface ErrorEnvelope {
  error: ApiErrorDto
}

export interface HealthDto {
  schema_version: SchemaVersion
  status: 'ok'
  version: string
}

export interface ProviderDto {
  id: string
  name: string
  media_kinds: MediaKind[]
  network: boolean
  optional: boolean
}

export interface ProvidersDto {
  schema_version: SchemaVersion
  providers: ProviderDto[]
}

export type ProviderId =
  | 'local'
  | 'tmdb'
  | 'bangumi'
  | 'anilist'
  | 'musicbrainz'
  | 'openlibrary'
export type OutputPreset = 'full' | 'metadata'
export type PlacementPolicy =
  | 'in_place'
  | 'symlink'
  | 'hardlink'
  | 'copy'
  | 'reflink'
export type ConflictPolicy = 'prefer_first' | 'review' | 'error'

export interface ProviderEndpoints {
  tmdb: string
  bangumi: string
  anilist: string
  musicbrainz: string
  openlibrary: string
  openlibrary_cover: string
}

export interface WorkspaceSettingsBase {
  offline: boolean
  proxy: string | null
  preferred_locales: string[]
  timeout_seconds: number
  auto_accept_confidence: number
  review_confidence: number
  output_preset: OutputPreset
  placement: PlacementPolicy
  conflict_policy: ConflictPolicy
  enabled_providers: ProviderId[]
  provider_endpoints: ProviderEndpoints
}

export interface SecretStatus {
  tmdb_api_token_configured: boolean
  anilist_access_token_configured: boolean
}

export interface WorkspaceSettings extends WorkspaceSettingsBase {
  secrets: SecretStatus
}

export interface UpdateWorkspaceSettingsRequest extends WorkspaceSettingsBase {
  tmdb_api_token: string | null
  anilist_access_token: string | null
  clear_tmdb_api_token: boolean
  clear_anilist_access_token: boolean
}

export interface SettingsEnvelope {
  schema_version: SchemaVersion
  settings: WorkspaceSettings
}

export interface RootSummary {
  id: string
  label: string
}

export interface RootsEnvelope {
  schema_version: SchemaVersion
  roots: RootSummary[]
}

export interface LibraryEntry {
  name: string
  path: string
  kind: 'directory' | 'file'
  size_bytes?: number
}

export interface LibraryEnvelope {
  schema_version: SchemaVersion
  root_id: string
  path: string
  entries: LibraryEntry[]
  truncated: boolean
}

export interface ListLibraryRequest {
  rootId: string
  path?: string
}

export interface SearchMatch {
  root_id: string
  path: string
  name: string
}

export interface SearchRequest {
  mediaKind: MediaKind
  query: string
  limit?: number
}

export interface SearchEnvelope {
  schema_version: SchemaVersion
  media_kind: MediaKind
  results: SearchMatch[]
  truncated: boolean
}

export interface ProviderProbeEnvelope {
  schema_version: SchemaVersion
  provider: ProviderId
  ok: boolean
  category: string
  message: string
}

export interface TemplateSample {
  title: string
  id: string
  year: number | null
  edition: string | null
}

export interface TemplatePreviewRequest {
  path_template: string
  content_template: string
  sample: TemplateSample
}

export interface TemplatePreviewEnvelope {
  schema_version: SchemaVersion
  path: string
  content: string
  content_bytes: number
}

export interface LoginRequest {
  password: string
}

export interface LoginResponse {
  schema_version: SchemaVersion
  csrf_token: string
  expires_at_ms: number
}

export interface CreateJobRequest {
  media_kind: MediaKind
  input_path: string
  apply: boolean
}

export interface JobInputDto extends CreateJobRequest {
  schema_version: SchemaVersion
}

export interface ProgressSummary {
  schema_version: SchemaVersion
  stage: string
  completed_items: number
  total_items: number | null
}

export interface ReviewSummary {
  schema_version: SchemaVersion
  candidate_count: number
  conflict_count: number
}

export interface ReviewDecisionDto {
  schema_version: SchemaVersion
  candidate_index: number
  accepted_conflict_indexes: number[]
}

export interface PlanSummary {
  schema_version: SchemaVersion
  operation_count: number
  requires_confirmation: boolean
  fingerprint?: string
}

export interface ExecutionSummary {
  schema_version: SchemaVersion
  completed_operations: number
  failed_operations: number
}

export interface JobDto {
  id: number
  input: JobInputDto
  state: JobState
  progress?: ProgressSummary
  review?: ReviewSummary
  review_decision?: ReviewDecisionDto
  plan?: PlanSummary
  execution?: ExecutionSummary
  created_at_ms: number
  updated_at_ms: number
}

export interface JobEnvelope {
  schema_version: SchemaVersion
  job: JobDto
}

export interface JobListEnvelope {
  schema_version: SchemaVersion
  jobs: JobDto[]
  has_more: boolean
}

export interface ListJobsRequest {
  limit?: number
  state?: JobState
}

export type MatchEvidenceKind = 'external_id' | 'title' | 'alias' | 'year' | 'sequence'

export interface ExternalIdArtifact {
  namespace: string
  value: string
}

export interface EvidenceArtifact {
  kind: MatchEvidenceKind
  points: number
  detail: string
}

export interface CandidateArtifact {
  index: number
  media_kind: MediaKind
  provider: string
  external_id: ExternalIdArtifact
  title: string
  year?: number
  sequence?: string
  score: number
  evidence: EvidenceArtifact[]
  evidence_truncated: boolean
}

export interface WarningArtifact {
  code: string
  message: string
}

export interface SourceArtifact {
  provider: string
  external_id?: ExternalIdArtifact
  locale?: string
}

export interface ConflictArtifact {
  index: number
  field_path: string
  message: string
  providers: string[]
  providers_truncated: boolean
  sources: SourceArtifact[]
  sources_truncated: boolean
}

export interface ReviewArtifactsEnvelope {
  schema_version: SchemaVersion
  job_id: number
  selected_candidate_index: number
  candidates: CandidateArtifact[]
  candidates_truncated: boolean
  warnings: WarningArtifact[]
  warnings_truncated: boolean
  conflicts: ConflictArtifact[]
  conflicts_truncated: boolean
}

export type OutputOperationKind =
  | 'create_directory'
  | 'write'
  | 'copy'
  | 'symlink'
  | 'hardlink'
  | 'reflink'

export interface OperationArtifact {
  index: number
  kind: OutputOperationKind
  source: string | null
  target: string
  content_bytes?: number
}

export interface PlanArtifactsEnvelope {
  schema_version: SchemaVersion
  job_id: number
  output_root: string
  operations: OperationArtifact[]
  operations_truncated: boolean
  requires_approval: boolean
}

export interface ReviewJobRequest {
  candidate_index: number
  accepted_conflict_indexes: number[]
}

export class ApiError extends Error {
  readonly status: number
  readonly code: string
  readonly details: Record<string, string> | undefined
  readonly requestId: string | undefined

  constructor(status: number, dto?: Partial<ApiErrorDto>) {
    super(dto?.message ?? `Request failed with status ${status}`)
    this.name = 'ApiError'
    this.status = status
    this.code = dto?.code ?? 'unexpected_response'
    this.details = dto?.details
    this.requestId = dto?.request_id
  }
}

type Fetch = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>

export interface ApiClientOptions {
  baseUrl?: string
  fetch?: Fetch
  csrfToken?: () => string | undefined
}

export class ApiClient {
  readonly #baseUrl: string
  readonly #fetch: Fetch
  readonly #csrfToken: () => string | undefined
  #issuedCsrfToken: string | undefined

  constructor(options: ApiClientOptions = {}) {
    this.#baseUrl = options.baseUrl ?? API_BASE
    this.#fetch = options.fetch ?? ((input, init) => globalThis.fetch(input, init))
    this.#csrfToken = options.csrfToken ?? (() => undefined)
  }

  health(): Promise<HealthDto> {
    return this.#request('/health')
  }

  providers(): Promise<ProvidersDto> {
    return this.#request('/providers')
  }

  settings(): Promise<SettingsEnvelope> {
    return this.#request('/settings')
  }

  updateSettings(
    request: UpdateWorkspaceSettingsRequest,
  ): Promise<SettingsEnvelope> {
    return this.#request('/settings', { method: 'PUT', body: request })
  }

  libraryRoots(): Promise<RootsEnvelope> {
    return this.#request('/library/roots')
  }

  listLibrary(request: ListLibraryRequest): Promise<LibraryEnvelope> {
    const query = new URLSearchParams({
      root_id: request.rootId,
      path: request.path ?? '',
    })
    return this.#request(`/library?${query.toString()}`)
  }

  search(request: SearchRequest): Promise<SearchEnvelope> {
    const query = new URLSearchParams({
      media_kind: request.mediaKind,
      query: request.query,
      limit: String(request.limit ?? 25),
    })
    return this.#request(`/search?${query.toString()}`)
  }

  testProvider(provider: ProviderId): Promise<ProviderProbeEnvelope> {
    return this.#request(`/providers/${encodeURIComponent(provider)}/test`, {
      method: 'POST',
      body: {},
    })
  }

  previewTemplate(
    request: TemplatePreviewRequest,
  ): Promise<TemplatePreviewEnvelope> {
    return this.#request('/templates/preview', {
      method: 'POST',
      body: request,
    })
  }

  async login(request: LoginRequest): Promise<LoginResponse> {
    const response = await this.#request<LoginResponse>('/auth/login', { method: 'POST', body: request })
    this.#issuedCsrfToken = response.csrf_token
    return response
  }

  async logout(): Promise<void> {
    await this.#request('/auth/logout', { method: 'POST' })
    this.#issuedCsrfToken = undefined
  }

  createJob(request: CreateJobRequest): Promise<JobEnvelope> {
    return this.#request('/jobs', { method: 'POST', body: request })
  }

  getJob(id: number): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}`)
  }

  listJobs(request: ListJobsRequest = {}): Promise<JobListEnvelope> {
    const query = new URLSearchParams()
    if (request.limit !== undefined) query.set('limit', String(request.limit))
    if (request.state !== undefined) query.set('state', request.state)
    const suffix = query.size === 0 ? '' : `?${query.toString()}`
    return this.#request(`/jobs${suffix}`)
  }

  retryJob(id: number): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/retry`, { method: 'POST' })
  }

  getJobReview(id: number, candidateIndex?: number): Promise<ReviewArtifactsEnvelope> {
    const suffix = candidateIndex === undefined ? '' : `?candidate_index=${candidateIndex}`
    return this.#request(`/jobs/${id}/review${suffix}`)
  }

  getJobPlan(id: number): Promise<PlanArtifactsEnvelope> {
    return this.#request(`/jobs/${id}/plan`)
  }

  cancelJob(id: number): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/cancel`, { method: 'POST' })
  }

  reviewJob(id: number, request: ReviewJobRequest): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/review`, { method: 'POST', body: request })
  }

  executeJob(id: number, idempotencyKey: string): Promise<JobEnvelope> {
    return this.#request(`/jobs/${id}/execute`, {
      method: 'POST',
      body: { approved: true },
      headers: { 'idempotency-key': idempotencyKey },
    })
  }

  async #request<T>(
    path: string,
    options: {
      method?: 'GET' | 'POST' | 'PUT'
      body?: unknown
      headers?: Record<string, string>
    } = {},
  ): Promise<T> {
    const method = options.method ?? 'GET'
    const headers: Record<string, string> = { ...options.headers }
    if (options.body !== undefined) headers['content-type'] = 'application/json'
    if (method !== 'GET') {
      const csrfToken = this.#csrfToken() ?? this.#issuedCsrfToken
      if (csrfToken) headers['x-csrf-token'] = csrfToken
    }

    const response = await this.#fetch(`${this.#baseUrl}${path}`, {
      method,
      credentials: 'same-origin',
      headers,
      ...(options.body === undefined ? {} : { body: JSON.stringify(options.body) }),
    })

    if (!response.ok) {
      const body = await readJson<Partial<ErrorEnvelope>>(response)
      throw new ApiError(response.status, body?.error)
    }
    if (response.status === 204) return undefined as T

    const body = await readJson<T>(response)
    if (body === undefined) throw new ApiError(response.status)
    return body
  }
}

async function readJson<T>(response: Response): Promise<T | undefined> {
  const contentType = response.headers.get('content-type') ?? ''
  if (!contentType.includes('application/json')) return undefined
  try {
    return (await response.json()) as T
  } catch {
    return undefined
  }
}

export const api = new ApiClient()
