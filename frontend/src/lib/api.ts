import type { Composition, CompositionRenderRequest } from './composition/types'
import type { CompositionTemplate } from './composition/templates'
import {
  parseProxyCreateResult,
  parseProxyList,
  parseProxyProfile,
  requireProxyKey,
  type ProxyCreateResult,
  type ProxyList,
  type ProxyProfile,
} from './proxy/types'
import type { Capabilities, EditState, Job, LutAsset, MediaEntry, MediaInfo, VideoInfo } from './types'

export type {
  ProxyArtifact,
  ProxyCodec,
  ProxyCreateResult,
  ProxyJobSummary,
  ProxyList,
  ProxyProfile,
} from './proxy/types'

const BACKEND_DOWN = 'Сервер недоступен. Запущен ли бэкенд? (cargo run на :8080)'

interface ApiErrorBody {
  error?: unknown
  code?: unknown
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

export class BackendUnavailableError extends Error {
  constructor() {
    super(BACKEND_DOWN)
    this.name = 'BackendUnavailableError'
  }
}

/** Fetch that distinguishes a network failure from a real HTTP error response. */
async function safeFetch(path: string, init?: RequestInit): Promise<Response> {
  try {
    return await fetch(path, init)
  } catch {
    throw new BackendUnavailableError()
  }
}

async function responseError(response: Response, fallback: string): Promise<ApiError> {
  const text = await response.text().catch(() => '')
  const trimmed = text.trim()
  let message = fallback
  let code: string | undefined

  if (trimmed) {
    try {
      const body = JSON.parse(trimmed) as ApiErrorBody
      if (typeof body.error === 'string' && body.error.trim()) message = body.error.trim()
      if (typeof body.code === 'string' && body.code.trim()) code = body.code.trim()
    } catch {
      message = trimmed
    }
  }

  return new ApiError(message, response.status, code)
}

async function requireOk(response: Response, fallback: string): Promise<void> {
  if (!response.ok) throw await responseError(response, fallback)
}

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await safeFetch(path, init)
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

async function postJson(path: string, body: unknown): Promise<{ jobId: string }> {
  return requestJson(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
}

export function importUrl(body: Record<string, unknown>): Promise<{ jobId: string }> {
  return postJson('/api/import', body)
}

export function edit(payload: unknown): Promise<{ jobId: string }> {
  return postJson('/api/edit', payload)
}

/** Upload local video/audio/image media; the backend probes it before storage. */
export async function uploadFile(file: File): Promise<MediaInfo> {
  const fd = new FormData()
  fd.append('file', file)
  const res = await safeFetch('/api/upload', { method: 'POST', body: fd })
  await requireOk(res, `upload -> HTTP ${res.status}`)
  return res.json()
}

/** Queue a schema-v1 multi-source composition render. */
export function renderComposition(request: CompositionRenderRequest, token?: string | null): Promise<{ jobId: string }> {
  return requestJson('/api/compositions/render', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    body: JSON.stringify(request),
  })
}

/** Upload and validate a 3D `.cube` LUT. */
export async function uploadLut(file: File, signal?: AbortSignal): Promise<LutAsset> {
  const fd = new FormData()
  fd.append('file', file)
  const res = await safeFetch('/api/luts', { method: 'POST', body: fd, signal })
  await requireOk(res, `LUT upload -> HTTP ${res.status}`)
  return res.json()
}

/** Resolve metadata for a previously stored immutable LUT asset. */
export async function getLut(id: string): Promise<LutAsset> {
  const res = await safeFetch(`/api/luts/${encodeURIComponent(id)}`)
  await requireOk(res, `LUT lookup -> HTTP ${res.status}`)
  return res.json()
}

export async function getJob(jobId: string): Promise<Job> {
  const res = await safeFetch(`/api/jobs/${jobId}`)
  await requireOk(res, `job poll -> HTTP ${res.status}`)
  return res.json()
}

export async function getCapabilities(): Promise<Capabilities> {
  const res = await safeFetch('/api/capabilities')
  await requireOk(res, `capabilities -> HTTP ${res.status}`)
  return res.json()
}

/** List persisted sources and outputs, newest first. */
export async function getLibrary(token?: string | null, spaceId?: string | null): Promise<MediaEntry[]> {
  const res = await safeFetch('/api/library', { headers: libraryAccessHeaders(token, spaceId) })
  await requireOk(res, `library -> HTTP ${res.status}`)
  return res.json()
}

/** Same-origin stable URL for a lazily loaded library thumbnail. */
export function libraryThumbnailUrl(id: string): string {
  if (!/^[A-Za-z0-9._-]{1,128}$/.test(id)) {
    throw new TypeError('Invalid library media id')
  }
  return '/api/library/' + encodeURIComponent(id) + '/thumbnail'
}

/** Same-origin stable URL for the bounded eight-cell video filmstrip. */
export function libraryFilmstripUrl(id: string): string {
  if (!/^[A-Za-z0-9._-]{1,128}$/.test(id)) {
    throw new TypeError('Invalid library media id')
  }
  return '/api/library/' + encodeURIComponent(id) + '/filmstrip'
}

/** Enqueue a content-addressed proxy for one original source video. */
export async function createLibraryProxy(
  sourceId: string,
  profile: ProxyProfile,
): Promise<ProxyCreateResult> {
  const path = `/api/library/${encodeURIComponent(sourceId)}/proxies`
  const raw = await requestJson<unknown>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(parseProxyProfile(profile)),
  })
  return parseProxyCreateResult(raw)
}

/** Return only verified proxies for the source's current fingerprint. */
export async function getLibraryProxies(sourceId: string): Promise<ProxyList> {
  const path = `/api/library/${encodeURIComponent(sourceId)}/proxies`
  return parseProxyList(await requestJson<unknown>(path), sourceId)
}

/** Cancel matching work and remove the source-owned proxy artifact. */
export async function deleteLibraryProxy(sourceId: string, key: string): Promise<void> {
  const safeKey = requireProxyKey(key)
  const path = `/api/library/${encodeURIComponent(sourceId)}/proxies/${safeKey}`
  const res = await safeFetch(path, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `proxy delete -> HTTP ${res.status}`)
  }
}

export interface LibraryMetadataPatch {
  title?: string | null
  favorite?: boolean
  tags?: string[]
}

export interface LibraryMetadataPut {
  title: string | null
  favorite: boolean
  tags: string[]
}

/** Update only the supplied local metadata fields for one library item. */
export function patchLibraryMetadata(
  id: string,
  metadata: LibraryMetadataPatch,
  token?: string | null,
  spaceId?: string | null,
): Promise<MediaEntry> {
  return requestJson(`/api/library/${encodeURIComponent(id)}/metadata`, {
    method: 'PATCH',
    headers: libraryAccessHeaders(token, spaceId, true),
    body: JSON.stringify(metadata),
  })
}

/** Replace all local metadata fields for one library item. */
export function putLibraryMetadata(
  id: string,
  metadata: LibraryMetadataPut,
  token?: string | null,
  spaceId?: string | null,
): Promise<MediaEntry> {
  return requestJson(`/api/library/${encodeURIComponent(id)}/metadata`, {
    method: 'PUT',
    headers: libraryAccessHeaders(token, spaceId, true),
    body: JSON.stringify(metadata),
  })
}

/** Delete a library entry (and its file on disk). */
export async function deleteLibraryItem(
  id: string,
  token?: string | null,
  spaceId?: string | null,
): Promise<void> {
  const res = await safeFetch(`/api/library/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    headers: libraryAccessHeaders(token, spaceId),
  })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `delete -> HTTP ${res.status}`)
  }
}

/** A saved editing project: a clip plus its persisted edit recipe. */
export interface ProjectDto {
  id: string
  name: string
  videoId: string
  video: VideoInfo
  edit: Partial<EditState>
  createdAt: number
  updatedAt: number
}

/** Create or update (keyed by videoId) the saved project for a clip. */
export async function saveProject(body: Record<string, unknown>): Promise<ProjectDto> {
  const res = await safeFetch('/api/projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

/** Fetch the saved project for a clip, or null if none exists yet. */
export async function getProjectByVideo(videoId: string): Promise<ProjectDto | null> {
  const res = await safeFetch(`/api/projects/by-video/${encodeURIComponent(videoId)}`)
  if (res.status === 404) return null
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

/** List saved projects, most recently updated first. */
export async function getProjects(): Promise<ProjectDto[]> {
  const res = await safeFetch('/api/projects')
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

export async function deleteProject(id: string): Promise<void> {
  const res = await safeFetch(`/api/projects/${id}`, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `projects -> HTTP ${res.status}`)
  }
}

export interface CompositionProjectDto {
  id: string
  spaceId?: string | null
  name: string
  schemaVersion: 2
  mode: 'composition'
  document: Composition
  sourceIds: string[]
  revision: number
  createdAt: number
  updatedAt: number
}

/** The API helper supplies the fixed project envelope fields. */
export interface CompositionProjectSaveRequest {
  name?: string
  spaceId?: string
  baseRevision?: number
  document: Composition
}

export interface CompositionProjectArchiveImportResponse {
  project: CompositionProjectDto
  sourceMapping: Record<string, string>
}

export type ProjectReviewRole = 'owner' | 'editor' | 'commenter' | 'viewer'

export interface ReviewCommentDto {
  id: string
  author: string
  body: string
  timelineTick: number
  parentId?: string | null
  createdAt: number
}

export interface ReviewThreadDto {
  id: string
  projectId: string
  comments: ReviewCommentDto[]
  resolvedAt?: number | null
  resolvedBy?: string | null
}

export interface ProjectReviewMemberDto {
  actor: string
  role: ProjectReviewRole
}

export interface ReviewAuditEventDto {
  id: string
  projectId: string
  actor: string
  action: string
  subjectId: string
  createdAt: number
}

export interface ReviewShareCreatedDto {
  grant: { id: string; projectId: string; expiresAt: number; revokedAt?: number | null }
  token: string
}

export interface SharedReviewDto {
  projectId: string
  projectName: string
  expiresAt: number
  threads: ReviewThreadDto[]
}

export interface AuthUserDto {
  id: string
  username: string
  createdAt: number
}

export interface AuthSessionDto {
  user: AuthUserDto
  token: string
  expiresAt: number
}

export type SpaceRole = 'owner' | 'editor' | 'viewer'

export interface SpaceDto {
  id: string
  name: string
  role: SpaceRole
  createdAt: number
  updatedAt: number
}

export interface SpaceMemberDto {
  actor: string
  role: SpaceRole
}

export interface SpaceInviteCreatedDto {
  id: string
  spaceId: string
  role: Exclude<SpaceRole, 'owner'>
  expiresAt: number
  token: string
}

export interface SpaceTemplateDto {
  id: string
  spaceId: string
  template: CompositionTemplate
  createdBy: string
  revision: number
  createdAt: number
  updatedAt: number
}

export interface BrandColorDto { name: string; value: string }
export interface BrandKitPayloadDto {
  colors: BrandColorDto[]
  fonts: Array<'Noto Sans' | 'Arial Unicode MS' | 'DejaVu Sans' | 'Arial'>
  logoSourceIds: string[]
}
export interface SpaceBrandKitDto {
  spaceId: string
  kit: BrandKitPayloadDto
  revision: number
  updatedBy: string | null
  updatedAt: number | null
}

export interface YouTubeConnectionStatusDto { configured: boolean; connected: boolean }

export type StockKind = 'photo' | 'video'
export interface StockAssetDto {
  providerId: number
  mediaType: 'image' | 'video'
  title: string
  author: string
  authorUrl: string
  sourcePageUrl: string
  previewUrl: string
  importUrl: string
  width: number
  height: number
  duration: number | null
}
export interface StockSearchResultDto {
  provider: 'Pexels'
  providerUrl: string
  page: number
  totalResults: number
  assets: StockAssetDto[]
}

/** Mirrors the backend's complete archive limit and fails before allocating FormData. */
export const MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES = 2 * 1024 * 1024 * 1024 + 4 * 1024 * 1024

function compositionProjectBody(body: CompositionProjectSaveRequest): Record<string, unknown> {
  return {
    schemaVersion: 2,
    mode: 'composition',
    ...(body.name === undefined ? {} : { name: body.name }),
    ...(body.spaceId === undefined ? {} : { spaceId: body.spaceId }),
    ...(body.baseRevision === undefined ? {} : { baseRevision: body.baseRevision }),
    document: body.document,
  }
}

export function createCompositionProject(
  body: CompositionProjectSaveRequest,
  token: string,
): Promise<CompositionProjectDto> {
  return requestJson('/api/composition-projects', {
    method: 'POST',
    headers: bearerHeaders(token, true),
    body: JSON.stringify(compositionProjectBody(body)),
  })
}

export function getCompositionProjects(token: string): Promise<CompositionProjectDto[]> {
  return requestJson('/api/composition-projects', { headers: bearerHeaders(token) })
}

export interface CompositionProjectListSnapshot {
  etag: string | null
  projects: CompositionProjectDto[]
}

export interface CompositionProjectChangeDto {
  projectId: string
  kind: 'upsert' | 'delete'
  revision?: number | null
}

/** Subscribe to membership-filtered project changes using the HttpOnly session cookie. */
export function subscribeCompositionProjectChanges(onChange: () => void): () => void {
  const source = new EventSource('/api/composition-projects/events', { withCredentials: true })
  source.addEventListener('project', onChange)
  source.addEventListener('resync', onChange)
  return () => source.close()
}

/** Conditionally refresh all visible projects, including membership/deletion changes. */
export async function getCompositionProjectsIfChanged(
  token: string,
  etag: string | null,
): Promise<CompositionProjectListSnapshot | undefined> {
  const headers = bearerHeaders(token)
  if (etag) headers['If-None-Match'] = etag
  const res = await safeFetch('/api/composition-projects', { headers })
  if (res.status === 304) return undefined
  await requireOk(res, `/api/composition-projects -> HTTP ${res.status}`)
  return { etag: res.headers.get('etag'), projects: await res.json() }
}

export async function getCompositionProject(id: string, token: string): Promise<CompositionProjectDto | null> {
  const path = `/api/composition-projects/${encodeURIComponent(id)}`
  const res = await safeFetch(path, { headers: bearerHeaders(token) })
  if (res.status === 404) return null
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

/** Poll one open project without transferring its document when the revision is unchanged. */
export async function getCompositionProjectIfChanged(
  id: string,
  token: string,
  revision: number | null,
): Promise<CompositionProjectDto | null | undefined> {
  const path = `/api/composition-projects/${encodeURIComponent(id)}`
  const headers = bearerHeaders(token)
  if (revision !== null) headers['If-None-Match'] = `"revision-${revision}"`
  const res = await safeFetch(path, { headers })
  if (res.status === 304) return undefined
  if (res.status === 404) return null
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

export function updateCompositionProject(
  id: string,
  body: CompositionProjectSaveRequest,
  token: string,
): Promise<CompositionProjectDto> {
  const path = `/api/composition-projects/${encodeURIComponent(id)}`
  return requestJson(path, {
    method: 'PUT',
    headers: bearerHeaders(token, true),
    body: JSON.stringify(compositionProjectBody(body)),
  })
}

export async function deleteCompositionProject(id: string, token: string): Promise<void> {
  const res = await safeFetch(`/api/composition-projects/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    headers: bearerHeaders(token),
  })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `composition project delete -> HTTP ${res.status}`)
  }
}

function bearerHeaders(token: string, json = false): Record<string, string> {
  return { Authorization: `Bearer ${token}`, ...(json ? { 'Content-Type': 'application/json' } : {}) }
}

function libraryAccessHeaders(
  token?: string | null,
  spaceId?: string | null,
  json = false,
): Record<string, string> {
  return {
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(spaceId ? { 'X-Space-Id': spaceId } : {}),
    ...(json ? { 'Content-Type': 'application/json' } : {}),
  }
}

export function getSpaces(token: string): Promise<SpaceDto[]> {
  return requestJson('/api/spaces', { headers: bearerHeaders(token) })
}

export function createSpace(token: string, name: string): Promise<SpaceDto> {
  return requestJson('/api/spaces', {
    method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ name }),
  })
}

export function renameSpace(spaceId: string, token: string, name: string, baseUpdatedAt: number): Promise<SpaceDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}`, {
    method: 'PATCH', headers: bearerHeaders(token, true), body: JSON.stringify({ name, baseUpdatedAt }),
  })
}

export async function deleteSpace(spaceId: string, token: string): Promise<void> {
  const response = await fetch(`/api/spaces/${encodeURIComponent(spaceId)}`, {
    method: 'DELETE', headers: bearerHeaders(token),
  })
  if (!response.ok) throw await responseError(response, `space delete -> HTTP ${response.status}`)
}

export function getSpaceMembers(spaceId: string, token: string): Promise<SpaceMemberDto[]> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/members`, { headers: bearerHeaders(token) })
}

export function createSpaceInvite(
  spaceId: string,
  token: string,
  role: Exclude<SpaceRole, 'owner'>,
  ttlSeconds = 7 * 24 * 60 * 60,
): Promise<SpaceInviteCreatedDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/invites`, {
    method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ role, ttlSeconds }),
  })
}

export function acceptSpaceInvite(inviteToken: string, token: string): Promise<SpaceDto> {
  return requestJson(`/api/space-invites/${encodeURIComponent(inviteToken)}/accept`, {
    method: 'POST', headers: bearerHeaders(token),
  })
}

export function setSpaceMember(
  spaceId: string,
  token: string,
  actor: string,
  role: Exclude<SpaceRole, 'owner'>,
): Promise<SpaceMemberDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/members/${encodeURIComponent(actor)}`, {
    method: 'PUT', headers: bearerHeaders(token, true), body: JSON.stringify({ role }),
  })
}

export async function removeSpaceMember(spaceId: string, token: string, actor: string): Promise<void> {
  const response = await fetch(`/api/spaces/${encodeURIComponent(spaceId)}/members/${encodeURIComponent(actor)}`, {
    method: 'DELETE', headers: bearerHeaders(token),
  })
  if (!response.ok) throw await responseError(response, `space member delete -> HTTP ${response.status}`)
}

export function transferSpaceOwnership(spaceId: string, token: string, targetActor: string): Promise<SpaceMemberDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/ownership-transfer`, {
    method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ targetActor }),
  })
}

export function getYouTubeConnectionStatus(token: string): Promise<YouTubeConnectionStatusDto> {
  return requestJson('/api/publish/youtube/status', { headers: bearerHeaders(token) })
}

export function beginYouTubeConnection(token: string): Promise<{ authorizationUrl: string }> {
  return requestJson('/api/publish/youtube/connect', { method: 'POST', headers: bearerHeaders(token, true), body: '{}' })
}

export async function disconnectYouTube(token: string): Promise<void> {
  const response = await safeFetch('/api/publish/youtube/connect', {
    method: 'DELETE', headers: bearerHeaders(token),
  })
  if (!response.ok) throw await responseError(response, `YouTube disconnect -> HTTP ${response.status}`)
}

export function publishYouTube(
  token: string,
  request: { outputId: string; title: string; description: string; privacyStatus: 'private' | 'unlisted' | 'public' },
): Promise<{ jobId: string }> {
  return requestJson('/api/publish/youtube', {
    method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify(request),
  })
}

export function getSpaceTemplates(spaceId: string, token: string): Promise<SpaceTemplateDto[]> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/templates`, {
    headers: bearerHeaders(token),
  })
}

export function createSpaceTemplate(
  spaceId: string,
  token: string,
  template: CompositionTemplate,
): Promise<SpaceTemplateDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/templates`, {
    method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ template }),
  })
}

export function updateSpaceTemplate(
  spaceId: string,
  templateId: string,
  token: string,
  baseRevision: number,
  template: CompositionTemplate,
): Promise<SpaceTemplateDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/templates/${encodeURIComponent(templateId)}`, {
    method: 'PUT', headers: bearerHeaders(token, true), body: JSON.stringify({ baseRevision, template }),
  })
}

export async function deleteSpaceTemplate(spaceId: string, templateId: string, token: string): Promise<void> {
  const response = await safeFetch(
    `/api/spaces/${encodeURIComponent(spaceId)}/templates/${encodeURIComponent(templateId)}`,
    { method: 'DELETE', headers: bearerHeaders(token) },
  )
  if (!response.ok) throw await responseError(response, `space template delete -> HTTP ${response.status}`)
}

export function getSpaceBrandKit(spaceId: string, token: string): Promise<SpaceBrandKitDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/brand-kit`, {
    headers: bearerHeaders(token),
  })
}

export function updateSpaceBrandKit(
  spaceId: string,
  token: string,
  baseRevision: number,
  kit: BrandKitPayloadDto,
): Promise<SpaceBrandKitDto> {
  return requestJson(`/api/spaces/${encodeURIComponent(spaceId)}/brand-kit`, {
    method: 'PUT', headers: bearerHeaders(token, true), body: JSON.stringify({ baseRevision, kit }),
  })
}

export function searchStockCatalog(
  token: string,
  query: string,
  kind: StockKind,
  orientation: '' | 'landscape' | 'portrait' | 'square' = '',
  page = 1,
): Promise<StockSearchResultDto> {
  const params = new URLSearchParams({ q: query, kind, page: String(page) })
  if (orientation) params.set('orientation', orientation)
  return requestJson(`/api/stock/search?${params}`, { headers: bearerHeaders(token) })
}

export function getProjectReviewThreads(projectId: string, token: string): Promise<ReviewThreadDto[]> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/reviews`, { headers: bearerHeaders(token) })
}

export function createProjectReviewThread(
  projectId: string,
  token: string,
  body: string,
  timelineTick: number,
): Promise<ReviewThreadDto> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/reviews`, {
    method: 'POST',
    headers: bearerHeaders(token, true),
    body: JSON.stringify({ body, timelineTick }),
  })
}

export function replyToProjectReviewThread(
  threadId: string,
  token: string,
  body: string,
): Promise<ReviewThreadDto> {
  return requestJson(`/api/review-threads/${encodeURIComponent(threadId)}/replies`, {
    method: 'POST',
    headers: bearerHeaders(token, true),
    body: JSON.stringify({ body }),
  })
}

export function setProjectReviewThreadResolved(
  threadId: string,
  token: string,
  resolved: boolean,
): Promise<ReviewThreadDto> {
  return requestJson(`/api/review-threads/${encodeURIComponent(threadId)}/resolution`, {
    method: 'PUT',
    headers: bearerHeaders(token, true),
    body: JSON.stringify({ resolved }),
  })
}

export function getProjectReviewMembers(projectId: string, token: string): Promise<ProjectReviewMemberDto[]> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/members`, { headers: bearerHeaders(token) })
}

export function setProjectReviewMember(
  projectId: string,
  token: string,
  actor: string,
  role: ProjectReviewRole,
): Promise<ProjectReviewMemberDto> {
  return requestJson(
    `/api/composition-projects/${encodeURIComponent(projectId)}/members/${encodeURIComponent(actor)}`,
    {
      method: 'PUT',
      headers: bearerHeaders(token, true),
      body: JSON.stringify({ role }),
    },
  )
}

export function transferCompositionProjectOwnership(
  projectId: string,
  token: string,
  targetActor: string,
): Promise<ProjectReviewMemberDto> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/ownership-transfer`, {
    method: 'POST',
    headers: bearerHeaders(token, true),
    body: JSON.stringify({ targetActor }),
  })
}

export function getProjectReviewAudit(projectId: string, token: string): Promise<ReviewAuditEventDto[]> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/review-audit`, { headers: bearerHeaders(token) })
}

export function createProjectReviewShare(
  projectId: string,
  token: string,
  ttlSeconds = 7 * 24 * 60 * 60,
): Promise<ReviewShareCreatedDto> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/review-shares`, {
    method: 'POST', headers: bearerHeaders(token, true),
    body: JSON.stringify({ ttlSeconds }),
  })
}

export function getSharedReview(token: string): Promise<SharedReviewDto> {
  return requestJson(`/api/review-shares/${encodeURIComponent(token)}`)
}

export function registerAuthUser(username: string, password: string): Promise<AuthSessionDto> {
  return requestJson('/api/auth/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ username, password }),
  })
}

export function loginAuthUser(username: string, password: string): Promise<AuthSessionDto> {
  return requestJson('/api/auth/login', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ username, password }),
  })
}

export function getAuthSession(token: string): Promise<AuthUserDto> {
  return requestJson('/api/auth/session', { headers: { Authorization: `Bearer ${token}` } })
}

export async function logoutAuthSession(token: string): Promise<void> {
  const path = '/api/auth/logout'
  const response = await safeFetch(path, { method: 'POST', headers: { Authorization: `Bearer ${token}` } })
  await requireOk(response, `${path} -> HTTP ${response.status}`)
}

export async function revokeProjectReviewShare(
  projectId: string,
  shareId: string,
  token: string,
): Promise<void> {
  const path = `/api/composition-projects/${encodeURIComponent(projectId)}/review-shares/${encodeURIComponent(shareId)}/revoke`
  const response = await safeFetch(path, {
    method: 'POST', headers: bearerHeaders(token),
  })
  await requireOk(response, `${path} -> HTTP ${response.status}`)
}

/**
 * Direct navigation keeps multi-gigabyte archives streaming to disk instead
 * of materializing them as an in-memory browser Blob.
 */
export function compositionProjectArchiveUrl(id: string): string {
  return `/api/composition-projects/${encodeURIComponent(id)}/archive`
}

/** Upload, verify and relink a portable `.veproj` into a new local project. */
export async function importCompositionProjectArchive(
  file: File,
  token: string,
  spaceId?: string,
): Promise<CompositionProjectArchiveImportResponse> {
  if (file.size > MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES) {
    throw new Error('Архив проекта превышает лимит 2 ГиБ')
  }
  const body = new FormData()
  body.append('file', file)
  const path = '/api/composition-projects/import'
  const headers = bearerHeaders(token)
  if (spaceId) headers['X-Space-Id'] = spaceId
  const res = await safeFetch(path, { method: 'POST', headers, body })
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

/** Ask the backend to cancel a running/pending job. Best-effort. */
export async function cancelJob(jobId: string): Promise<void> {
  try {
    await fetch(`/api/jobs/${jobId}/cancel`, { method: 'POST' })
  } catch {
    // The poll loop will surface the resulting state.
  }
}

/**
 * Poll a job until it finishes. Resolves with the completed job (status `done`),
 * rejects with the job's error, or rejects with Error('cancelled') so callers
 * can tell a user cancellation apart from a real failure.
 */
export function pollJob(jobId: string, onTick?: (job: Job) => void): Promise<Job> {
  return new Promise((resolve, reject) => {
    const tick = async () => {
      try {
        const job = await getJob(jobId)
        onTick?.(job)
        if (job.status === 'done') return resolve(job)
        if (job.status === 'cancelled') return reject(new Error('cancelled'))
        if (job.status === 'interrupted') {
          return reject(new Error('Задача прервана (сервер перезапущен)'))
        }
        if (job.status === 'error') {
          return reject(new Error(job.error || 'задача завершилась с ошибкой'))
        }
        setTimeout(tick, 500)
      } catch (e) {
        reject(e)
      }
    }
    void tick()
  })
}
