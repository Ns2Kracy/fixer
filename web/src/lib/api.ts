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
      method?: 'GET' | 'POST'
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
