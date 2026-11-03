import type { Composition, CompositionDspEffect, CompositionRenderRequest, WireVideoEffect } from './composition/types'
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

export async function getJob(jobId: string, signal?: AbortSignal): Promise<Job> {
  const res = await safeFetch(`/api/jobs/${jobId}`, { signal })
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

/** Suggest a crop rect that removes letterbox / pillarbox bars. */
export async function detectLibraryCrop(
  id: string,
  token?: string | null,
  spaceId?: string | null,
): Promise<{ x: number; y: number; w: number; h: number }> {
  return requestJson(`/api/library/${encodeURIComponent(id)}/cropdetect`, {
    method: 'POST',
    headers: libraryAccessHeaders(token, spaceId, true),
  })
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

export function getCompositionProjects(token: string,signal?:AbortSignal): Promise<CompositionProjectDto[]> {
  return requestJson('/api/composition-projects', { headers: bearerHeaders(token),...(signal?{signal}:{}) })
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

export type FreezeCompositionProjectResult = { changed: false } | {
  changed: true
  project: Omit<CompositionProjectDto, 'document'> & { document: Composition | NativeNestedProject }
  media: MediaEntry
}

/** Saves a native freeze against exactly the displayed server revision. */
export function freezeCompositionProject(id: string, body: { revision: number; compositionId?: string; clipId: string; time: number }, token: string): Promise<FreezeCompositionProjectResult> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(id)}/freeze-frame`, {
    method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify(body),
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

const MAX_POLL_ATTEMPTS = 3600; // 50 minutes max (500ms * 3600)
const POLL_TIMEOUT_MS = 50 * 60 * 1000; // 50 minutes

/** Resolve a job status, rejecting after timeout or max attempts to prevent hangs. */
export function pollJob(jobId: string, onTick?: (job: Job) => void): Promise<Job> {
  return new Promise((resolve, reject) => {
    let attempt = 0;
    const startTime = Date.now();
    
    const tick = async () => {
      try {
        // Check timeout before each poll
        if (attempt >= MAX_POLL_ATTEMPTS || Date.now() - startTime > POLL_TIMEOUT_MS) {
          return reject(new Error('Job polling timed out'));
        }
        
        const job = await getJob(jobId);
        attempt++;
        onTick?.(job);
        
        if (job.status === 'done') return resolve(job);
        if (job.status === 'cancelled') return reject(new Error('cancelled'));
        if (job.status === 'interrupted') {
          return reject(new Error('Задача прервана (сервер перезапущен)'));
        }
        if (job.status === 'error') {
          return reject(new Error(job.error || 'задача завершилась с ошибкой'));
        }
        
        // Exponential backoff: 500ms → 1s → 2s → 4s → max 5s
        const delay = Math.min(500 * Math.pow(2, Math.min(attempt - 1, 4)), 5000);
        setTimeout(tick, delay);
      } catch (e) {
        reject(e);
      }
    };
    void tick();
  });
}

export async function scanLibraryShots(id: string, token?: string | null, spaceId?: string | null): Promise<import('./analysis/shots.js').ShotScan> {
  const result = await requestJson<unknown>(`/api/library/${encodeURIComponent(id)}/shots`, { method: 'POST', headers: libraryAccessHeaders(token, spaceId, true) })
  const validateShotScan: typeof import('./analysis/shots.js').validateShotScan = (await import('./analysis/shots.js')).validateShotScan
  validateShotScan(result)
  return result
}
export async function measureLibraryShots(id: string, spans: readonly import('./analysis/shots.js').ShotSpan[], token?: string | null, spaceId?: string | null, expectedSourceHash?: string): Promise<import('./analysis/shots.js').ShotMeasurement[]> {
  return requestJson(`/api/library/${encodeURIComponent(id)}/shots/measure`, { method: 'POST', headers: libraryAccessHeaders(token, spaceId, true), body: JSON.stringify({ spans, expectedSourceHash }) })
}
export async function compareLibraryFrames(id: string, leftTick: number, rightTick: number, token?: string | null, spaceId?: string | null): Promise<{ leftTick: number; rightTick: number; similarity: number; motion: number }> {
  return requestJson(`/api/library/${encodeURIComponent(id)}/compare-frames`, { method: 'POST', headers: libraryAccessHeaders(token, spaceId, true), body: JSON.stringify({ leftTick, rightTick }) })
}
export async function getLibraryAnalysisFrame(id: string, tick: number, token?: string | null, spaceId?: string | null): Promise<Blob> {
  const response = await safeFetch(`/api/library/${encodeURIComponent(id)}/analysis-frame?tick=${tick}`, { headers: libraryAccessHeaders(token, spaceId) })
  if (!response.ok) throw await responseError(response, 'Кадр недоступен')
  return response.blob()
}

type NativeMotionPath = { readonly nodes: readonly import('./composition/types').CompositionPathNode[] }
type NativePathPoint = import('./composition/types').CompositionPathPoint
export type NativeMotionPathRequest =
  | { operation: 'nearest';path: NativeMotionPath;point: NativePathPoint }
  | { operation: 'translate';path: NativeMotionPath;delta: NativePathPoint }
  | { operation: 'remove' | 'toggle_segment'; path: NativeMotionPath; index: number }
  | { operation: 'measure'; path: NativeMotionPath }
  | { operation: 'solve'; path: NativeMotionPath }
  | { operation: 'sample'; path: NativeMotionPath; progresses: readonly number[] }
  | { operation: 'edit'; path: NativeMotionPath; index: number; part: 'point' | 'in_handle' | 'out_handle'; value: NativePathPoint }
  | { operation: 'mode'; path: NativeMotionPath; index: number; mode: 'corner' | 'smooth' | 'auto' }
  | { operation: 'insert'; path: NativeMotionPath; segment: number; t: number; id: string }
export interface NativeMotionPathResponse {
  path: NativeMotionPath
  length: number
  points: readonly NativePathPoint[]
  nodeProgresses?: readonly number[]
  nearest?: { segment: number;t: number;distance: number }
}
/** Geometry, authored solve and path sampling are executed by native Rust. */
export function evaluateMotionPath(request: NativeMotionPathRequest, signal?: AbortSignal): Promise<NativeMotionPathResponse> {
  return requestJson('/api/compositions/motion-path',{ method: 'POST',headers: { 'Content-Type': 'application/json' },body: JSON.stringify(request),signal })
}

export type NativeConversionSegment = { kind: 'Linear' | 'Hold' | 'Spline' } | { kind: 'Elastic';dir: 'In' | 'Out' | 'InOut';amplitude: number;period: number } | { kind: 'Bounce';dir: 'In' | 'Out' | 'InOut' }
export interface NativeConversionTrack {
  sampleGrid?: { timeBase: number;rateNum: number;rateDen: number;offsetNum: number }
  keys: readonly { t_us: number;value: number;in_: readonly [number,number];out: readonly [number,number];segment: NativeConversionSegment }[]
  extrapolate: { before: 'Hold' | 'Continue' | 'Loop' | 'PingPong' | 'Offset';after: 'Hold' | 'Continue' | 'Loop' | 'PingPong' | 'Offset' }
}
export interface NativeConversionNode { point: NativePathPoint;incoming: NativePathPoint;outgoing: NativePathPoint;cubic: boolean }
export interface NativePositionConversionRequest {
  nodes: readonly NativeConversionNode[] | null
  tracks: readonly NativeConversionTrack[]
  options: { fpsNum: number;fpsDen: number;startFrame: number;endFrame: number;tolerancePx: number;everyFrames: number;xyMode: 'editable' | 'bake' }
}
export interface NativePositionConversionResult {
  nodes: readonly NativeConversionNode[] | null
  tracks: readonly NativeConversionTrack[]
  sampleCount: number;nodeCount: number;maxErrorPx: number;checkCount: number;withinTolerance: boolean;limit: string | null
}
export function previewPositionConversion(request: NativePositionConversionRequest,signal?: AbortSignal): Promise<NativePositionConversionResult> {
  return requestJson('/api/compositions/position-conversion',{ method: 'POST',headers: { 'Content-Type': 'application/json' },body: JSON.stringify(request),signal })
}

export interface NativeColor { readonly r: number;readonly g: number;readonly b: number;readonly a: number }
export interface NativeColorTrack {
  readonly keys: readonly {
    readonly t_us: number
    readonly value: NativeColor
    readonly in_: readonly [number,number]
    readonly out: readonly [number,number]
    readonly segment: NativeConversionSegment
    readonly legacy_curve?: 'ease_in_out'
    readonly tangent_modes?: {readonly incoming:'auto'|'free';readonly outgoing:'auto'|'free';readonly continuity:'smooth'|'broken'}
  }[]
  readonly extrapolate: NativeConversionTrack['extrapolate']
}
export function sampleAnimatedColors(request: { readonly track: NativeColorTrack;readonly default: NativeColor;readonly times_us: readonly number[] },signal?: AbortSignal): Promise<{ colors: NativeColor[] }> {
  return requestJson('/api/compositions/animated-color',{ method: 'POST',headers: { 'Content-Type': 'application/json' },body: JSON.stringify(request),signal })
}

export interface NativeCompositionInstance {
  animationOffsetUs?: number
  id: string
  timing: { composition: string;timelineStartUs: number;sourceInUs: number;sourceOutUs: number }
  transform: import('./composition/types').WireTransform
  opacity: import('./composition/types').WireColorClip['opacity']
  blendMode: import('./composition/types').CompositionBlendMode
}
export interface NativeProjectLut {readonly id:string;readonly name:string;readonly lut:import("./composition/types").WireColorLut;readonly sourcePath?:string}
export interface NativeProjectFont {readonly family:string;readonly data:string}
export interface NativeProjectMediaItem { librarySourceId?:string; source: import('./composition/types').CompositionSource; name: string; placeholder?: boolean; concatMetadata?:Readonly<Record<string,unknown>> }
export interface NativeNestedProject {
  projectMedia?: Readonly<Record<string,NativeProjectMediaItem>>
  fonts?:readonly NativeProjectFont[]
  timelineOrder?: readonly string[]
  luts?:readonly NativeProjectLut[]
  schemaVersion: 1
  root: string
  compositions: Record<string,{ durationUs?: number;content: import('./composition/types').WireComposition;instances: readonly NativeCompositionInstance[] }>
}
export interface NativeTemplateInfo {
 schemaVersion:1
 compositionId:string
 width:number
 height:number
 slots:readonly {mediaId:string;name:string;kind:import('./composition/types').CompositionSource['kind'];seconds:number}[]
}
export async function inspectNativeTemplate(project:NativeNestedProject,compositionId:string,signal?:AbortSignal):Promise<NativeTemplateInfo>{
 const reply=await requestJson<NativeTemplateInfo>('/api/compositions/nested-template-info',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,compositionId}),signal})
 if(signal?.aborted)throw new DOMException('Aborted','AbortError')
 const expected=Object.entries(project.projectMedia??{}).filter(([,item])=>item.placeholder===true).map(([id])=>id)
 if(reply?.schemaVersion!==1||reply.compositionId!==compositionId||!Array.isArray(reply.slots)||reply.slots.length!==expected.length||new Set(reply.slots.map(slot=>slot.mediaId)).size!==expected.length||reply.slots.some(slot=>!expected.includes(slot.mediaId)||!Number.isFinite(slot.seconds)||slot.seconds<0||slot.kind!==project.projectMedia?.[slot.mediaId]?.source.kind))throw new Error('Native template inspection returned invalid slots')
 return reply
}
export async function setNativeProjectMediaPlaceholder(project: NativeNestedProject, sourceId: string, placeholder: boolean, signal?: AbortSignal): Promise<NativeNestedProject> {
  const reply = await requestJson<{ project: NativeNestedProject; applied: boolean }>('/api/compositions/nested-media-placeholder', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project, sourceId, placeholder }), signal })
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (!reply || typeof reply.applied !== 'boolean' || !reply.project || reply.project.root !== project.root || reply.project.schemaVersion !== 1) throw new Error('Native media placeholder returned invalid project')
  return reply.applied ? reply.project : project
}
export async function fillNativeProjectMedia(project:NativeNestedProject,mediaId:string,sourceId:string,token:string,signal?:AbortSignal):Promise<NativeNestedProject>{
 const reply=await requestJson<{schemaVersion:number;applied:boolean;project:NativeNestedProject}>('/api/compositions/nested-media-fill',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({project,mediaId,sourceId}),signal})
 if(signal?.aborted)throw new DOMException('Aborted','AbortError')
 const item=reply?.project?.projectMedia?.[mediaId]
 if(reply?.schemaVersion!==1||reply.applied!==true||reply.project.schemaVersion!==1||reply.project.root!==project.root||!item||item.placeholder===true||item.source.id!==mediaId||item.librarySourceId!==sourceId)throw new Error('Native FillSlot returned invalid project')
 return reply.project
}
export async function removeNativeProjectMedia(project: NativeNestedProject, sourceId: string, signal?: AbortSignal): Promise<NativeNestedProject> {
  const reply = await requestJson<{ project: NativeNestedProject; applied: boolean }>('/api/compositions/nested-media-remove', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project, sourceId }), signal })
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (!reply || typeof reply.applied !== 'boolean' || !reply.project || reply.project.root !== project.root || reply.project.schemaVersion !== 1) throw new Error('Native media removal returned invalid project')
  return reply.applied ? reply.project : project
}
export interface NativeNestedFrameResponse {
  entries: readonly { composition: string;sourceTimeUs: number;canvasToRoot: readonly [number,number,number,number,number,number];instancePath: readonly string[];ancestors: readonly NativeCompositionInstance[];ancestorPoses: readonly { localTimeUs: number;x: number;y: number;scaleX: number;scaleY: number;rotationDegrees: number;opacity: number }[] }[]
}
export function evaluateNestedFrame(project: NativeNestedProject,timeUs: number,signal?: AbortSignal): Promise<NativeNestedFrameResponse> {
  return requestJson('/api/compositions/nested-frame',{ method: 'POST',headers: { 'Content-Type': 'application/json' },body: JSON.stringify({ project,timeUs }),signal })
}

export type NestedCompositionProjectDto = Omit<CompositionProjectDto,'document'> & { document: NativeNestedProject }
export type NestedCompositionProjectSaveRequest = Omit<CompositionProjectSaveRequest,'document'> & { document: NativeNestedProject }
export function createNestedCompositionProject(body: NestedCompositionProjectSaveRequest,token: string): Promise<NestedCompositionProjectDto> {
  return requestJson('/api/composition-projects',{ method: 'POST',headers: bearerHeaders(token,true),body: JSON.stringify({ ...body,schemaVersion: 2,mode: 'composition' }) })
}
export function updateNestedCompositionProject(id: string,body: NestedCompositionProjectSaveRequest,token: string): Promise<NestedCompositionProjectDto> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(id)}`,{ method: 'PUT',headers: bearerHeaders(token,true),body: JSON.stringify({ ...body,schemaVersion: 2,mode: 'composition' }) })
}
export function getNestedCompositionProject(id: string,token: string): Promise<NestedCompositionProjectDto> {
  return requestJson(`/api/composition-projects/${encodeURIComponent(id)}`,{ headers: bearerHeaders(token) })
}

export function groupNestedComposition(project: NativeNestedProject,parent: string,child: string,instanceId: string,signal?: AbortSignal): Promise<NativeNestedProject> {
  return requestJson('/api/compositions/nested-group',{ method: 'POST',headers: { 'Content-Type': 'application/json' },body: JSON.stringify({ project,parent,child,instanceId }),signal })
}

export interface NativeNestedPreviewFrame {
  width: number
  height: number
  rgba: Uint8ClampedArray
}

export async function renderNestedPreview(
  project: NativeNestedProject,
  timeUs: number,
  renderedOutputs: Readonly<Record<string, string>>,
  token: string,
  signal?: AbortSignal,
  compositionId?: string,
  savedProject?: {projectId:string;expectedRevision:number},
): Promise<NativeNestedPreviewFrame> {
  let response: Response
  try {
    response = await fetch('/api/compositions/nested-preview', {
      method: 'POST',
      headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' },
      body: JSON.stringify({ project, timeUs, renderedOutputs, compositionId, savedProject }),
      signal,
    })
  } catch (error) {
    if (signal?.aborted) throw error
    throw new BackendUnavailableError()
  }
  await requireOk(response, 'Не удалось загрузить кадр композиции')
  const width = Number(response.headers.get('x-frame-width'))
  const height = Number(response.headers.get('x-frame-height'))
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 || width > 3840 || height > 2160) {
    throw new ApiError('Некорректный размер кадра композиции', response.status)
  }
  const bytes = await response.arrayBuffer()
  if (bytes.byteLength !== width * height * 4) {
    throw new ApiError('Неполный RGBA-кадр композиции', response.status)
  }
  return { width, height, rgba: new Uint8ClampedArray(bytes) }
}

/** Encode the committed native frame in Rust; preserves preview ACL and saved-mask binding. */
export async function exportNestedFramePng(project:NativeNestedProject,timeUs:number,renderedOutputs:Readonly<Record<string,string>>,token:string,signal?:AbortSignal,compositionId?:string,savedProject?:{projectId:string;expectedRevision:number}):Promise<Blob>{
 const response=await safeFetch('/api/compositions/nested-preview',{method:'POST',headers:{...bearerHeaders(token),'Content-Type':'application/json',Accept:'image/png'},body:JSON.stringify({project,timeUs,renderedOutputs,compositionId,savedProject}),signal})
 await requireOk(response,'Не удалось сохранить кадр')
 if(response.headers.get('content-type')?.split(';')[0]!=='image/png')throw new ApiError('Сервер не вернул PNG',response.status)
 const blob=await response.blob()
 if(blob.size<8||blob.size>64*1024*1024)throw new ApiError('Недопустимый размер PNG',response.status)
 return blob
}

export function splitNestedComposition(project: NativeNestedProject, parent: string, instanceId: string, rightId: string, timeUs: number, signal?: AbortSignal): Promise<NativeNestedProject> {
  return requestJson('/api/compositions/nested-split', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ project, parent, instanceId, rightId, timeUs }), signal,
  })
}

/** Graph samples use the same Rust interpolation as the rendered frame. */
export async function sampleConcatCurve(points:readonly (readonly[number,number])[],signal?:AbortSignal):Promise<readonly number[]>{
 const response=await requestJson<{values:number[]}>('/api/compositions/concat-curve',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({points,samples:256}),signal})
 if(!Array.isArray(response.values)||response.values.length!==256||response.values.some(v=>!Number.isFinite(v)))throw new Error('Некорректный результат кривой Concat')
 return response.values
}
export async function sampleToneCurve(points: readonly (readonly [number, number])[], signal?: AbortSignal, periodic = false): Promise<readonly number[]> {
  const response = await requestJson<{ values: number[] }>('/api/compositions/tone-curve', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ points, samples: 256, ...(periodic ? { periodic } : {}) }), signal })
  if (!Array.isArray(response.values) || response.values.length !== 256 || response.values.some((value) => !Number.isFinite(value))) throw new Error('Некорректный результат кривой')
  return response.values
}


export function nestedDurations(project: NativeNestedProject, signal?: AbortSignal): Promise<Record<string, number>> {
  return requestJson('/api/compositions/nested-durations', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project }), signal,
  })
}
export function exportNestedVideo(
  project: NativeNestedProject, compositionId: string, startUs: number, endUs: number,
  renderedOutputs: Readonly<Record<string, string>>, token: string, signal?: AbortSignal, mode: 'video_only' | 'video_audio' = 'video_only',
  savedProject?: {projectId:string;expectedRevision:number},
): Promise<{ jobId: string; mode: 'video_only' | 'video_audio'; container: 'mkv' | 'mp4' }> {
  return requestJson('/api/compositions/nested-export', {
    method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({ schemaVersion: 1, mode, project, compositionId, startUs, endUs, renderedOutputs, savedProject }), signal,
  })
}

export function validateColorLut(lut: import('./composition/types').WireColorLut, signal?: AbortSignal): Promise<{title:string;shaperSize:number;cubeSize:number}> {
 return requestJson('/api/compositions/color-lut',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(lut),signal})
}

export interface NativeColorMatch {readonly shadows:readonly [number,number];readonly midtones:readonly [number,number];readonly highlights:readonly [number,number];readonly lightness:readonly [number,number,number];readonly saturation:number;readonly before:number;readonly after:number}
export interface ColorMatchFrame {readonly width:number;readonly height:number;readonly rgba:readonly number[]}
export function solveColorMatch(current:ColorMatchFrame,reference:ColorMatchFrame,parameters:import('./composition/types').WireLumetri,skinProtect:boolean,signal?:AbortSignal,luts:readonly NativeProjectLut[]=[]):Promise<NativeColorMatch> {
 return requestJson('/api/compositions/color-match',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({current,reference,parameters,skinProtect,luts}),signal})
}

export interface NativeBuiltinLut {readonly id:string;readonly label:string;readonly input:boolean;readonly size:number}
export function listBuiltinLuts(signal?:AbortSignal):Promise<readonly NativeBuiltinLut[]> {return requestJson('/api/compositions/builtin-luts',{signal})}

export function importProjectLut(project:NativeNestedProject,lut:import('./composition/types').WireColorLut,name:string,sourcePath:string,composition:string,track:string,clip:string,effectIndex:number,input:boolean,signal?:AbortSignal):Promise<{project:NativeNestedProject;id:string;reused:boolean}> {
 return requestJson('/api/compositions/project-lut-import',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,lut,name,sourcePath,composition,track,clip,effectIndex,input}),signal})
}

export function exportColorLut(source:{reference:string;luts?:readonly NativeProjectLut[]}|{lut:import('./composition/types').WireColorLut},format:'cube'|'three_dl',signal?:AbortSignal):Promise<{text:string;filename:string}> {
 return requestJson('/api/compositions/lut-export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({...source,format}),signal})
}

export function updateProjectLut(project:NativeNestedProject,id:string,changes:{name?:string;lut?:import('./composition/types').WireColorLut;sourcePath?:string},signal?:AbortSignal):Promise<NativeNestedProject> {
 return requestJson('/api/compositions/project-lut-update',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,id,...changes}),signal})
}

export function removeProjectLut(project:NativeNestedProject,id:string,signal?:AbortSignal):Promise<NativeNestedProject> {
 return requestJson('/api/compositions/project-lut-remove',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,id}),signal})
}

export type NativeScopeKind='waveform'|'parade'|'histogram'|'vectorscopeYuv'|'vectorscopeHls'
export interface NativeScopeOptions {colorSpace?:'auto'|'rec601'|'rec709'|'rec2100';layoutWidth?:number;layoutHeight?:number;scale?:'bits8'|'float'|'hdr';targets?:'75'|'100';scopes?:readonly NativeScopeKind[];matrix?:'Bt601'|'Bt709'|'Bt2020Ncl';clamp?:boolean;rows?:number;vectorSize?:number;waveformType?:'rgb'|'luma'|'yc'|'ycNoChroma';paradeType?:'rgb'|'yuv'|'rgbWhite';brightness?:'dimmed'|'normal'|'bright';columns?:number;peaks?:number}
export interface NativeScopeLevel {min:number;max:number;mean:number}
export type NativeScopeOverlay = {kind:'line';x1:number;y1:number;x2:number;y2:number;color:string}|{kind:'circle';cx:number;cy:number;r:number;color:string}|{kind:'rect';x:number;y:number;width:number;height:number;color:string}|{kind:'text';x:number;y:number;text:string;color:string}
export interface NativeScopeGraph {title?:string;axisUnit?:string;foreground?:readonly Extract<NativeScopeOverlay,{kind:'line'}>[];axis?:readonly {position:number;left:string;right:string}[];overlay?:readonly NativeScopeOverlay[];image:{width:number;height:number;rgba:readonly number[]}}
export type NativeScopes = {resolvedColorSpace?:'rec601'|'rec709'|'rec2100';layout?:{columns:number;rows:number};samples:readonly [number,number];matrix:string;unit:string;stats:{r:NativeScopeLevel;g:NativeScopeLevel;b:NativeScopeLevel;y:NativeScopeLevel;chromaMean:readonly [number,number];hueDeg:number;saturation:number}|null} & Partial<Record<NativeScopeKind,NativeScopeGraph>>
export function analyzeScopes(frame:ColorMatchFrame,options:NativeScopeOptions={},signal?:AbortSignal):Promise<NativeScopes> {
 return requestJson('/api/compositions/scopes',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({frame,options}),signal})
}

export function analyzeScopesBinary(frame:NativeNestedPreviewFrame,options:NativeScopeOptions={},signal?:AbortSignal):Promise<NativeScopes> {
 return requestJson('/api/compositions/scopes-rgba',{method:'POST',headers:{'Content-Type':'application/octet-stream','X-Frame-Width':String(frame.width),'X-Frame-Height':String(frame.height),'X-Scope-Options':JSON.stringify(options)},body:frame.rgba.slice().buffer,signal})
}

export type NativeAudioDspUnit = 'none' | 'decibels' | 'hertz' | 'milliseconds' | 'seconds' | 'percent' | 'q' | 'ratio' | 'semitones' | 'bpm' | 'pan' | 'toggle' | 'choice'
export interface NativeAudioDspParameter {
  readonly step?: number | null
  readonly animatable?: boolean
  readonly id: string
  readonly name: string
  readonly min: number
  readonly max: number
  readonly default: number
  readonly unit: NativeAudioDspUnit
  readonly logScale: boolean
  readonly choices: readonly string[]
}
export interface NativeAudioDspEffect {
  readonly nativeExport: boolean
  readonly id: string
  readonly name: string
  readonly category: 'filter' | 'dynamics' | 'time' | 'channel' | 'restoration' | 'pitch' | 'modulation' | 'reverb' | 'special'
  readonly params: readonly NativeAudioDspParameter[]
}
export interface NativeAudioDspCatalog {
  readonly schemaVersion: 1
  readonly effects: readonly NativeAudioDspEffect[]
  readonly projectCatalog?: FilmcraftAudioProjectCatalog
}
export interface FilmcraftAudioProjectParameter {
  readonly id: string
  readonly label: string
  readonly default: { Float: number } | { Choice: number } | { Bool: boolean }
  readonly kind: 'Bool' | { Choice: readonly string[] } | { Float: { min: number; max: number; decimals: number; soft_min: number; soft_max: number; unit: string } }
  readonly animatable: boolean
}
export interface FilmcraftAudioProjectCatalog {
  readonly schemaVersion: 1
  readonly sourceCommit: string
  readonly effects: readonly {
    readonly id: string
    readonly name: string
    readonly intrinsic: boolean
    readonly kind: 'Audio'
    readonly category: readonly string[]
    readonly params: readonly FilmcraftAudioProjectParameter[]
  }[]
}
export interface FilmcraftAudioProjectResponseRequest {
  readonly effect: string
  readonly params?: Readonly<Record<string, number>>
  readonly animation?: CompositionDspEffect['animation']
  readonly localTimeSeconds?: number
  readonly sampleRate: number
  readonly frequencies?: readonly number[]
  readonly levels?: readonly number[]
  readonly band?: number
}
export interface FilmcraftAudioProjectResponse {
  readonly dspId: string
  readonly params: Readonly<Record<string, number>>
  readonly latencySamples: number
  readonly prerollSamples: number
  readonly responseDb: readonly (number | null)[]
  readonly transferDb: readonly (number | null)[]
}
/** Metadata adaptation only; all project→DSP signal conversions remain native. */
export function audioDspEditorEffects(catalog: NativeAudioDspCatalog): readonly NativeAudioDspEffect[] {
  return [...catalog.effects, ...(catalog.projectCatalog?.effects ?? []).filter(effect => !effect.intrinsic).map(effect => ({
    id: `filmcraft:${effect.id}`, name: `Filmcraft · ${effect.name}`, nativeExport: true,
    category: 'special' as const,
    params: effect.params.map(param => {
      const float = typeof param.kind === 'object' && 'Float' in param.kind ? param.kind.Float : undefined
      const choices = typeof param.kind === 'object' && 'Choice' in param.kind ? param.kind.Choice : []
      const toggle = param.kind === 'Bool'
      const value = 'Float' in param.default ? param.default.Float : 'Choice' in param.default ? param.default.Choice : Number(param.default.Bool)
      return { id: param.id, animatable: param.animatable, name: `${param.label}${float?.unit ? ` (${float.unit})` : ''}`, min: float?.min ?? 0,
        max: float?.max ?? (toggle ? 1 : Math.max(0, choices.length - 1)), default: value,
        unit: (toggle ? 'toggle' : choices.length ? 'choice' : 'none') as NativeAudioDspUnit,
        choices, logScale: false }
    }),
  }))]
}
/** Native configured editor characteristics; null denotes unsupported response type. */
export function getAudioProjectResponse(input: FilmcraftAudioProjectResponseRequest, signal?: AbortSignal): Promise<FilmcraftAudioProjectResponse> {
  return requestJson('/api/compositions/audio-project-response', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}
/** Registry metadata; listing an effect does not imply it is bound to the project renderer. */
export function getAudioDspCatalog(signal?: AbortSignal): Promise<NativeAudioDspCatalog> {
  return requestJson('/api/compositions/audio-dsp-catalog', { signal })
}

/** Character formatting and text replacement use the native Filmcraft run rules. */
export type GraphicTextParameters = Extract<import('./composition/types').WireVideoEffect,{kind:'graphic_text'}>['parameters']
export type GraphicCharacterStyle = NonNullable<GraphicTextParameters['runs']>[number]['style']
export type GraphicTextEditOperation = {kind:'normalize'} | {kind:'apply';start:number;end:number;style:GraphicCharacterStyle} | {kind:'clear';start:number;end:number} | {kind:'adjust';newText:string}
export async function editGraphicTextRuns(text:string,runs:NonNullable<GraphicTextParameters['runs']>,operation:GraphicTextEditOperation,signal?:AbortSignal):Promise<NonNullable<GraphicTextParameters['runs']>> {
 const response=await requestJson<{runs:NonNullable<GraphicTextParameters['runs']>}>('/api/compositions/graphic-text-edit',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({text,runs,operation}),signal})
 if(!Array.isArray(response.runs))throw new Error('Некорректный результат форматирования текста')
 return response.runs
}

/** Capture canvas-space edge distances with the native animated graphic evaluator. */
export async function captureGraphicPin(graphic: {readonly layers: readonly import('./composition/types').GraphicLayer[]}, width:number, height:number, localUs:number, uid:number, to:import('./composition/types').GraphicPin['to'], edges:readonly [boolean,boolean,boolean,boolean], signal?:AbortSignal):Promise<import('./composition/types').GraphicPin> {
  const response = await requestJson<{pin:import('./composition/types').GraphicPin}>('/api/compositions/graphic-pin-capture', {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({graphic,width,height,localUs,uid,to,edges}),signal})
  if (!response.pin || response.pin.offsets?.length !== 4 || !response.pin.offsets.every(Number.isFinite)) throw new Error('Некорректные расстояния привязки')
  return response.pin
}

export async function mapGraphicClock(responsiveTime:import('./composition/types').GraphicResponsiveTime,relativeUs:number,durationUs:number,animationOffsetUs:number,signal?:AbortSignal):Promise<number> {
  const response=await requestJson<{localUs:number}>('/api/compositions/graphic-clock',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({responsiveTime,relativeUs,durationUs,animationOffsetUs}),signal})
  if (!Number.isSafeInteger(response.localUs)) throw new Error('Некорректное время графического слоя')
  return response.localUs
}

export interface NativeGraphicTemplateControl {
  readonly id:string; readonly name:string; readonly kind:'text'|'font'|'slider'|'checkbox'|'color'|'position'; readonly layer:number; readonly param:string; readonly min?:number|null; readonly max?:number|null
}
export interface NativeGraphicTemplate {
  readonly resources?: {readonly fonts?:readonly {readonly family:string;readonly style:string;readonly license:string;readonly file:string;readonly data:string}[]}
  readonly id:string; readonly name:string; readonly category:string; readonly description:string; readonly author:string; readonly license:string; readonly tags:readonly string[]; readonly canvas:readonly [number,number]; readonly durationUs:number
  readonly graphic:{readonly layers:readonly import('./composition/types').GraphicLayer[]}; readonly roll?:import('./composition/types').GraphicRoll|null; readonly responsiveTime?:import('./composition/types').GraphicResponsiveTime|null; readonly controls:readonly NativeGraphicTemplateControl[]
}
export function getGraphicTemplates(signal?:AbortSignal):Promise<readonly NativeGraphicTemplate[]> {
  return requestJson('/api/compositions/graphic-templates',{signal})
}
export function editGraphicTemplate(template:NativeGraphicTemplate,control:string,value:unknown,signal?:AbortSignal,clock?:GraphicTemplateClock):Promise<NativeGraphicTemplate> {
  return requestJson('/api/compositions/graphic-template-edit',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({template,control,value,clock}),signal})
}

/** Native .fcgt document boundary; source fields and resources remain intact. */
export function readGraphicTemplateFile(contents:string,signal?:AbortSignal):Promise<Record<string,unknown>> {
  return requestJson('/api/compositions/graphic-template-file/read',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({contents}),signal})
}
export function writeGraphicTemplateFile(document:Record<string,unknown>,signal?:AbortSignal):Promise<string> {
  return requestJson('/api/compositions/graphic-template-file/write',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(document),signal})
}

export interface UserGraphicTemplateFile {file:string;document:Record<string,unknown>}
export function listUserGraphicTemplates(signal?:AbortSignal):Promise<UserGraphicTemplateFile[]> {
 return requestJson('/api/compositions/graphic-template-library',{signal})
}
export function installUserGraphicTemplate(contents:string,signal?:AbortSignal,options?:{replace?:boolean}):Promise<string> {
 return requestJson('/api/compositions/graphic-template-library/install',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({contents,...options}),signal})
}
export function removeUserGraphicTemplate(file:string,signal?:AbortSignal):Promise<{removed:boolean}> {
 return requestJson('/api/compositions/graphic-template-library/remove',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({file}),signal})
}

export function instantiateGraphicTemplateFile(document:Record<string,unknown>,signal?:AbortSignal):Promise<NativeGraphicTemplate> {
 return requestJson('/api/compositions/graphic-template-file/instantiate',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(document),signal})
}

export interface GraphicTemplateClock {relativeUs:number;durationUs:number;animationOffsetUs:number}
export function getGraphicTemplateValues(template:NativeGraphicTemplate,clock:GraphicTemplateClock,signal?:AbortSignal):Promise<Record<string,unknown>> {
 return requestJson('/api/compositions/graphic-template-values',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({template,clock}),signal})
}

export function exportGraphicTemplateFile(template:NativeGraphicTemplate,animationOffsetUs:number,signal?:AbortSignal,options?:{readonly embedFonts?:boolean;readonly fontLicense?:string;readonly autoTextControls?:boolean;readonly controls?:readonly {readonly layer:number|string;readonly param?:string;readonly kind?:string;readonly id?:string;readonly name?:string;readonly min?:number;readonly max?:number}[]}):Promise<string> {
 return requestJson('/api/compositions/graphic-template-file/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({template,animationOffsetUs,...options}),signal})
}

export function editGraphicTemplateBatch(template:NativeGraphicTemplate,values:Readonly<Record<string,unknown>>,signal?:AbortSignal,clock?:GraphicTemplateClock):Promise<NativeGraphicTemplate> {
  return requestJson('/api/compositions/graphic-template-edit-batch',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({template,values,clock}),signal})
}

export interface GraphicTemplateThumbnail {readonly width:number;readonly height:number;readonly pngBase64:string}
export function getGraphicTemplateThumbnail(template:NativeGraphicTemplate,width=160,signal?:AbortSignal):Promise<GraphicTemplateThumbnail> {
 return requestJson('/api/compositions/graphic-template-thumbnail',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({template,width}),signal})
}
export function getGraphicTemplateDocumentThumbnail(document:Readonly<Record<string,unknown>>,width=160,signal?:AbortSignal):Promise<GraphicTemplateThumbnail> {
 return requestJson('/api/compositions/graphic-template-thumbnail',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({document,width}),signal})
}

export interface CompositionFontUsage {readonly family:string;readonly style:string;readonly count:number;readonly missing?:boolean}
export function getCompositionFonts(composition:import('./composition/types').WireComposition,signal?:AbortSignal):Promise<readonly CompositionFontUsage[]> {
 return requestJson('/api/compositions/fonts-used',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(composition),signal})
}

export type CompositionFontSelection=string|{readonly family:string;readonly style?:string}
export function replaceCompositionFonts(composition:import('./composition/types').WireComposition,from:CompositionFontSelection,to:CompositionFontSelection,signal?:AbortSignal):Promise<{readonly composition:import('./composition/types').WireComposition;readonly replaced:number}> {
 return requestJson('/api/compositions/replace-fonts',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({composition,from,to}),signal})
}

export function editProjectFontFile(project:NativeNestedProject,operation:{kind:'add';data:string}|{kind:'remove';index:number},signal?:AbortSignal):Promise<NativeNestedProject>{
 return requestJson('/api/compositions/project-font-file',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,operation}),signal})
}

export function getProjectFonts(project:NativeNestedProject,signal?:AbortSignal):Promise<readonly CompositionFontUsage[]> {
 return requestJson('/api/compositions/project-fonts-used',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(project),signal})
}
export function replaceProjectFonts(project:NativeNestedProject,from:CompositionFontSelection,to:CompositionFontSelection,signal?:AbortSignal):Promise<{readonly project:NativeNestedProject;readonly replaced:number}> {
 return requestJson('/api/compositions/project-replace-fonts',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,from,to}),signal})
}

/** Return a new native document; the caller commits it through normal editor history. */
export function upgradeCompositionCaptions(composition:import('./composition/types').WireComposition,captionIds:readonly string[],signal?:AbortSignal):Promise<import('./composition/types').WireComposition> {
  return requestJson('/api/compositions/captions/upgrade',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({composition,captionIds}),signal})
}

export type CaptionCommandTimeInput = {readonly unit:'frames'|'seconds';readonly value:number}|{readonly unit:'timecode'|'ticks';readonly value:string}
export type CaptionEditOperation =
 | {readonly kind:'add';readonly trackId:string;readonly startTick?:number|string;readonly endTick?:number|string;readonly startInput?:CaptionCommandTimeInput;readonly endInput?:CaptionCommandTimeInput;readonly text?:string;readonly speaker?:string|null}
 | {readonly kind:'split';readonly id:string;readonly timeTick?:number|string;readonly timeInput?:CaptionCommandTimeInput}
 | {readonly kind:'merge';readonly ids:readonly string[]}
 | {readonly kind:'text';readonly id:string;readonly text?:string;readonly speaker?:string|null}
 | {readonly kind:'times';readonly id:string;readonly startTick?:number|string;readonly endTick?:number|string;readonly startInput?:CaptionCommandTimeInput;readonly endInput?:CaptionCommandTimeInput}
 | {readonly kind:'trim';readonly id:string;readonly inEdge:boolean;readonly deltaTicks:number|string}
 | {readonly kind:'move';readonly ids:readonly string[];readonly deltaTicks:number|string}
 | {readonly kind:'delete';readonly ids:readonly string[];readonly ripple?:boolean}
export interface CaptionEditResponse {readonly composition:import('./composition/types').WireComposition;readonly appliedDelta:number|string|null;readonly deleted:number}
export interface CaptionEditContext {readonly playheadTick?:number|string;readonly dropFrame?:boolean}
export function editCompositionCaptions(composition:import('./composition/types').WireComposition,operation:CaptionEditOperation,signal?:AbortSignal,context?:CaptionEditContext):Promise<CaptionEditResponse> {
 return requestJson('/api/compositions/captions/edit',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({composition,operation,...context}),signal})
}

/** Filmcraft file time is signed 254016000000-Hz ticks, encoded as decimal strings. */
export interface CaptionFileCue {
 readonly startTick:string;readonly endTick:string;readonly text:string
 readonly speaker?:string|null;readonly id?:string|null;readonly settings?:string
}
export interface CaptionFileDocument {
 readonly ticksPerSecond:'254016000000';readonly cues:readonly CaptionFileCue[]
 readonly blocks?:readonly string[];readonly warnings?:readonly string[]
}
export type CaptionFileFormat='srt'|'vtt'|'scc'|'mcc'|'stl'|'ttml'|'dfxp'
export function readCaptionFile(dataBase64:string,options?:{readonly format?:CaptionFileFormat;readonly extension?:string;readonly snapToRate?:{readonly num:number;readonly den:number}},signal?:AbortSignal):Promise<{readonly format:CaptionFileFormat;readonly document:CaptionFileDocument}> {
 return requestJson('/api/compositions/captions/file/read',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({dataBase64,...options}),signal})
}
export function writeCaptionFile(document:CaptionFileDocument,format:CaptionFileFormat,options?:{readonly dropFrame?:boolean;readonly rate?:{readonly num:number;readonly den:number};readonly offsetTick?:string},signal?:AbortSignal):Promise<{readonly extension:CaptionFileFormat;readonly dataBase64:string}> {
 return requestJson('/api/compositions/captions/file/write',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({document,format,...options}),signal})
}

export interface PreparedCaptionFileImport {
 readonly name:string;readonly format:'Subtitle'|'CEA-608'|'CEA-708'|'Teletext';readonly language:string
 readonly enabled:boolean;readonly locked:boolean;readonly syncLock:boolean;readonly backgroundColor:readonly [number,number,number,number]
 readonly document:CaptionFileDocument
}
export function prepareCaptionFileImport(dataBase64:string,path:string,rate:{readonly num:number;readonly den:number},options?:{readonly name?:string;readonly format?:CaptionFileFormat},signal?:AbortSignal):Promise<PreparedCaptionFileImport> {
 return requestJson('/api/compositions/captions/file/prepare-import',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({dataBase64,path,rate,...options}),signal})
}

/** Source JSON stays text so Filmcraft u64 IDs and ticks survive browser transport. */
export interface FilmcraftAppliedTransition { id: string; trackId: string; trackKind: string; effect: string; reverse: boolean; startTicks: string; durationTicks: string; paramsText: string }
export const FILMCRAFT_LABELS = ['Violet', 'Iris', 'Caribbean', 'Lavender', 'Cerulean', 'Forest', 'Rose', 'Mango', 'Purple', 'Blue', 'Teal', 'Magenta', 'Tan', 'Green', 'Brown', 'Yellow'] as const
export type FilmcraftLabel = typeof FILMCRAFT_LABELS[number]
export type FilmcraftLabelCommand = 'edit.label' | `edit.label.${Lowercase<FilmcraftLabel>}`
export type FilmcraftProjectItemKind = 'Media' | 'Sequence' | 'Subclip' | 'Graphic' | 'AdjustmentLayer'
export interface FilmcraftSubclipInfo { parentId: string; startTicks: string; endTicks: string; restrictTrims: boolean }
export interface FilmcraftMarks { in: string | null; out: string | null; split: { videoIn: string | null; videoOut: string | null; audioIn: string | null; audioOut: string | null } }
export interface FilmcraftImportResult {
  projectBins?: Array<{ id: string; name: string; parentId: string | null; depth: number; itemIds?: string[] }>
  labelPalette?: Array<{ name: FilmcraftLabel; rgb: number[] }>
  projectItems?: Array<{ id: string; name: string; kind: FilmcraftProjectItemKind; mediaInterpretation?: FilmcraftMediaInterpretation | null; subclip?: FilmcraftSubclipInfo | null; sourceMarks?: FilmcraftMarks; label?: FilmcraftLabel }>
  documentText: string
  originalSchema: number
  sequences: Array<{ tracks?: Array<{ id: string; name: string; kind: 'video_tracks' | 'audio_tracks'; locked: boolean }>; marks?: FilmcraftMarks; id: string; name: string; settings: unknown; sampleRate: number; durationSeconds: number; durationFrames?: string; transitions?: FilmcraftAppliedTransition[]; clips?: Array<{ id: string; name: string; label?: FilmcraftLabel; trackId: string; trackKind: string; locked: boolean; linkId?: string | null; startTicks: string; durationTicks: string }> }>
  media: Array<{ id: string; name: string; path: string; offline: boolean }>
}
export function importFilmcraftProject(documentText: string, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  return requestJson('/api/compositions/filmcraft/import', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ document: documentText }), signal,
  })
}
export interface FilmcraftAudioExport {
  document: string
  sequenceId: string
  startSample: number
  frames: number
  channels: 1 | 2
  bits: 16 | 24
  format: 'wav' | 'aiff'
  sources: Record<string, string>
}
export async function exportFilmcraftAudio(input: FilmcraftAudioExport, token?: string | null, signal?: AbortSignal): Promise<Blob> {
  if (!Number.isSafeInteger(input.startSample) || !Number.isSafeInteger(input.frames)) {
    throw new Error('Недопустимый диапазон звука')
  }
  const headers: Record<string, string> = { 'Content-Type': 'application/json' }
  if (token) headers.Authorization = `Bearer ${token}`
  const response = await safeFetch('/api/compositions/filmcraft/export-audio', {
    method: 'POST', headers, body: JSON.stringify(input), signal,
  })
  await requireOk(response, 'Не удалось экспортировать звук проекта')
  return response.blob()
}

export type FilmcraftTrimRequest = {
  document: string
  sequenceId: string
  clipId: string
  edge: 'in' | 'out'
  mode?: 'regular' | 'ripple'
  linked: boolean
} & ({ deltaTicks: string; deltaFrames?: never } | { deltaFrames: string; deltaTicks?: never })
export interface FilmcraftTrimResult extends FilmcraftImportResult { appliedDeltaTicks: string }
export function trimFilmcraftProject(input: FilmcraftTrimRequest, signal?: AbortSignal): Promise<FilmcraftTrimResult> {
  return requestJson('/api/compositions/filmcraft/trim', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

/** Frame delta is in source media, using the selected sequence frame clock. */
export function slipFilmcraftProject(input: { document: string; sequenceId: string; clipId: string; deltaFrames: string }, signal?: AbortSignal): Promise<FilmcraftTrimResult> {
  return requestJson('/api/compositions/filmcraft/slip', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function stretchFilmcraftProject(input: { document: string; sequenceId: string; clipId: string; edge: 'in' | 'out'; deltaFrames: string }, signal?: AbortSignal): Promise<FilmcraftImportResult & { appliedSpeed: number }> {
  return requestJson('/api/compositions/filmcraft/rate-stretch', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function rollFilmcraftProject(input: { document: string; sequenceId: string; leftClipId: string; rightClipId: string; deltaFrames: string }, signal?: AbortSignal): Promise<FilmcraftTrimResult> {
  return requestJson('/api/compositions/filmcraft/roll', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function slideFilmcraftProject(input: { document: string; sequenceId: string; clipId: string; deltaFrames: string }, signal?: AbortSignal): Promise<FilmcraftTrimResult> {
  return requestJson('/api/compositions/filmcraft/slide', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function deleteFilmcraftClips(input: { document: string; sequenceId: string; clipIds: string[]; ripple: boolean }, signal?: AbortSignal): Promise<FilmcraftImportResult & { removed: number; closedSpans: Array<{ startTicks: string; durationTicks: string }> }> {
  return requestJson('/api/compositions/filmcraft/delete', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function closeFilmcraftGap(input: { document: string; sequenceId: string; trackId: string; timeFrames: string }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  return requestJson('/api/compositions/filmcraft/close-gap', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function razorFilmcraftProject(input: { document: string; sequenceId: string; timeFrames: string } & ({ clipIds: string[]; trackIds?: never } | { trackIds: string[]; clipIds?: never }), signal?: AbortSignal): Promise<FilmcraftImportResult & { createdClipIds: string[] }> {
  return requestJson('/api/compositions/filmcraft/razor', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function removeFilmcraftRange(input: { document: string; sequenceId: string; trackIds: string[]; startFrames: string; durationFrames: string; ripple: boolean }, signal?: AbortSignal): Promise<FilmcraftImportResult & { removedClipIds: string[] }> {
  return requestJson('/api/compositions/filmcraft/remove-range', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export function placeFilmcraftClips(input: { document: string; sequenceId: string; placements: Array<{ trackId: string; clipText: string }>; insert: boolean }, signal?: AbortSignal): Promise<FilmcraftImportResult & { placedClipIds: string[] }> {
  return requestJson('/api/compositions/filmcraft/place-clips', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal,
  })
}

export interface FilmcraftClipboardResult { clipboardText: string; clipCount: number; markerCount: number }
export interface FilmcraftPasteResult extends FilmcraftImportResult { selectedClipIds: string[]; playheadTicks: string; playheadFrames: string }
export function copyFilmcraftClips(input: { document: string; sequenceId: string; clipIds: string[]; linked: boolean; includeMarkers: boolean }, signal?: AbortSignal): Promise<FilmcraftClipboardResult> {
  return requestJson('/api/compositions/filmcraft/copy', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}
export function pasteFilmcraftClips(input: { document: string; sequenceId: string; clipboardText: string; insert: boolean; includeMarkers: boolean; rippleMarkers: boolean } & ({ timeFrames: string; timeTicks?: never } | { timeTicks: string; timeFrames?: never }), signal?: AbortSignal): Promise<FilmcraftPasteResult> {
  return requestJson('/api/compositions/filmcraft/paste', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export function addFilmcraftTransition(input: { document: string; sequenceId: string; trackId: string; transitionText: string }, signal?: AbortSignal): Promise<FilmcraftImportResult & { createdTransitionId: string }> {
  return requestJson('/api/compositions/filmcraft/add-transition', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export interface FilmcraftTransitionDuration { value: number; unit: 'frames' | 'seconds' }
export function applyFilmcraftTransition(input: { document: string; sequenceId: string; kind: 'video' | 'audio'; effect: string; targetedTrackIds?: string[]; clipId?: string; edge?: 'in' | 'out'; params?: Record<string, unknown>; reverse?: boolean } & ({ frames: string; duration?: never } | { frames?: never; duration?: FilmcraftTransitionDuration }) & ({ timeTicks: string; timeFrames?: never } | { timeFrames: string; timeTicks?: never }), signal?: AbortSignal): Promise<FilmcraftImportResult & { createdTransitionId: string }> {
  return requestJson('/api/compositions/filmcraft/apply-transition', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export interface FilmcraftTransitionParameter { id: string; label: string; kind: string | Record<string, unknown>; default: Record<string, unknown> }
export interface FilmcraftTransitionDefinition { id: string; name: string; kind: 'VideoTransition' | 'AudioTransition'; category: string[]; params: FilmcraftTransitionParameter[] }
export interface FilmcraftTransitionCatalogEntry { definition: FilmcraftTransitionDefinition; instance: unknown }
export function getFilmcraftTransitions(signal?: AbortSignal): Promise<FilmcraftTransitionCatalogEntry[]> {
  return requestJson('/api/compositions/filmcraft/transitions', { signal })
}

export function setFilmcraftTransition(input: { document: string; sequenceId: string; transitionId: string; reset?: boolean; params?: Record<string, unknown>; reverse?: boolean }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  return requestJson('/api/compositions/filmcraft/set-transition', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export function applyFilmcraftSelectionTransitions(input: { document: string; sequenceId: string; clipIds: string[]; editPoints?: { clipId: string; edge: 'in' | 'out' }[]; videoEffect: string; audioEffect: string } & ({ videoFrames: string; videoDuration?: never } | { videoFrames?: never; videoDuration?: FilmcraftTransitionDuration }) & ({ audioFrames: string; audioDuration?: never } | { audioFrames?: never; audioDuration?: FilmcraftTransitionDuration }), signal?: AbortSignal): Promise<FilmcraftImportResult & { createdTransitionIds: string[] }> {
  return requestJson('/api/compositions/filmcraft/batch-transitions', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export type FilmcraftMonitorFrame = { kind?: 'generator' | 'image' | 'video' | 'sequence'; sourcePts?: string; originPts?: string; ordinal?: string; timeBase?: { num: string; den: string }; sampleTicks?: string; width: number; height: number; ptsTicks: string; rgbaHex: string }
export type FilmcraftTrimMonitorSide = {
  clip: string; item: string; name: string; track: string; video: boolean
  time: string; mediaTime: string; mediaFrame: string; sourceIn: string; sourceOut: string
  frame?: FilmcraftMonitorFrame; frameUnavailable?: string
}
export type FilmcraftTrimMonitor = {
  editTime: string | null; rollPair: [string, string] | null
  outgoing: FilmcraftTrimMonitorSide | null; incoming: FilmcraftTrimMonitorSide | null
}
export function getFilmcraftTrimMonitor(input: { document: string; sequenceId: string; clipId: string; edge: 'in' | 'out'; includeFrames?: boolean; sources?: Record<string,string> }, signal?: AbortSignal, token?: string | null): Promise<FilmcraftTrimMonitor> {
  return requestJson('/api/compositions/filmcraft/trim-monitor', { method: 'POST', headers: { ...(token ? bearerHeaders(token) : {}), 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export type FilmcraftNearestEditKind = 'trimIn' | 'trimOut' | 'rippleIn' | 'rippleOut' | 'roll'
export type FilmcraftEditPoint = { clip: string; out: boolean; kind: 'trim' | 'ripple' | 'roll' }
export function getFilmcraftNearestEditPoints(input: { document: string; sequenceId: string; kind: FilmcraftNearestEditKind; targetedTrackIds?: string[] } & ({ timeTicks: string; timeFrames?: never } | { timeTicks?: never; timeFrames: string }), signal?: AbortSignal): Promise<{ editPoints: FilmcraftEditPoint[] }> {
  return requestJson('/api/compositions/filmcraft/nearest-edit-points', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export type FilmcraftTrimShift = { outShift: string; inShift: string }
export type FilmcraftTrimPointsResult = FilmcraftImportResult & { shift: FilmcraftTrimShift; shiftFrames: FilmcraftTrimShift } & ({ appliedTicks: string[]; refusal: null } | { appliedTicks: null; refusal: string })
/** A command refusal can include earlier successful edits; callers must adopt documentText. */
export function trimFilmcraftEditPoints(input: { document: string; sequenceId: string; points: FilmcraftEditPoint[]; linked: boolean; shift?: FilmcraftTrimShift } & ({ deltaTicks: string; deltaFrames?: never; playheadTicks?: never; playheadFrames?: never } | { deltaTicks?: never; deltaFrames: string; playheadTicks?: never; playheadFrames?: never } | { deltaTicks?: never; deltaFrames?: never; playheadTicks: string; playheadFrames?: never } | { deltaTicks?: never; deltaFrames?: never; playheadTicks?: never; playheadFrames: string }), signal?: AbortSignal): Promise<FilmcraftTrimPointsResult> {
  return requestJson('/api/compositions/filmcraft/trim-points', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export type FilmcraftToPlayheadResult = FilmcraftImportResult & { rangeTicks: [string, string]; playheadTicks: string; playheadFrames: string; trackIds: string[] }
/** Q/W uses selected linked clips first, otherwise explicitly targeted tracks. */
export function trimFilmcraftToPlayhead(input: { document: string; sequenceId: string; clipIds: string[]; targetedTrackIds: string[]; side: 'previous' | 'next'; ripple: boolean; linked: boolean } & ({ timeTicks: string; timeFrames?: never } | { timeTicks?: never; timeFrames: string }), signal?: AbortSignal): Promise<FilmcraftToPlayheadResult> {
  return requestJson('/api/compositions/filmcraft/to-playhead', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export type FilmcraftTrimSessionAction =
  | { op: 'shuttle'; clock: number; direction: 'forward' | 'reverse'; slow: boolean }
  | { op: 'tick'; clock: number }
  | { op: 'stop'; clock?: number }
  | { op: 'cancel' }
  | { op: 'around'; clock: number; loop: boolean; toggle: boolean; centerPlayhead: boolean; prerollSeconds: number; postrollSeconds: number }
export type FilmcraftTrimSessionResult = FilmcraftImportResult & {
  sessionId: string; revision: string; shift: FilmcraftTrimShift; shiftFrames: FilmcraftTrimShift
  status: { playing: boolean; playheadTicks: string; playheadFrames: string; dynamic: { offsetFrames: string; speed: number; atLimit: boolean } | null; around: { startTicks: string; endTicks: string; loop: boolean } | null }
  commit: { committed: boolean; label: string | null; offsetTicks: string; offsetFrames: string } | null
}
export function createFilmcraftTrimSession(token: string, input: { creationKey?: string; document: string; sequenceId: string; points: FilmcraftEditPoint[]; linked: boolean; shift: FilmcraftTrimShift } & ({ playheadTicks: string; playheadFrames?: never } | { playheadTicks?: never; playheadFrames: string }), signal?: AbortSignal): Promise<FilmcraftTrimSessionResult> {
  return requestJson('/api/compositions/filmcraft/trim-sessions/create', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}
export function readFilmcraftTrimSession(token: string, sessionId: string, signal?: AbortSignal): Promise<FilmcraftTrimSessionResult> {
  return requestJson('/api/compositions/filmcraft/trim-sessions/read', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify({ sessionId }), signal })
}
export function commandFilmcraftTrimSession(token: string, input: { sessionId: string; revision: string; action: FilmcraftTrimSessionAction }, signal?: AbortSignal): Promise<FilmcraftTrimSessionResult> {
  return requestJson('/api/compositions/filmcraft/trim-sessions/command', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}
export function closeFilmcraftTrimSession(token: string, sessionId: string, revision: string, signal?: AbortSignal): Promise<{ closed: true }> {
  return requestJson('/api/compositions/filmcraft/trim-sessions/close', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify({ sessionId, revision }), signal })
}

export type FilmcraftPreviewInput = { sources?: Record<string, string>; document: string; sequenceId: string; timeFrames?: string; timeTicks?: string; effects?: boolean; captions?: boolean }
export type FilmcraftPreviewFrame = FilmcraftMonitorFrame & { kind: 'sequence'; sequenceId: string; scale: number; effects: boolean; captions: boolean }
export type FilmcraftExportPreset = 'lossless' | 'adaptive_high' | 'adaptive_medium' | 'adaptive_low' | 'high_quality1080' | 'high_quality2160' | 'youtube1080' | 'youtube2160' | 'vimeo1080' | 'vimeo2160' | 'social_vertical' | 'mjpeg' | 'hevc' | 'prores_proxy' | 'prores_lt' | 'prores_standard' | 'prores_hq' | 'dnxhr_lb' | 'dnxhr_sq' | 'dnxhr_hq' | 'apv42210' | 'apv42212' | 'mxf_dnxhr_hq' | 'mxf_prores_hq' | 'mxf_atom_dnxhr_hq' | 'mxf_h264' | 'png_sequence' | 'tiff_sequence' | 'bmp_sequence' | 'gif' | 'wav16' | 'wav24' | 'aiff16'
export interface FilmcraftLibraryPreset { name: string; category: string; description: string; settings: Record<string, unknown>; builtin: boolean; favorite: boolean }
export interface FilmcraftPresetFile { format: 'filmcraft.export-presets'; version: number; presets: Array<Omit<FilmcraftLibraryPreset, 'builtin' | 'favorite'>>; favorites: string[] }
export interface FilmcraftPresetSaveOverrides { format?: string; width?: number; height?: number; fps?: number | string; scale?: number; audio?: boolean; quality?: number; bitrateKbps?: number; maxBitrateKbps?: number; bitrateMode?: 'cbr' | 'vbr1Pass' | 'vbr2Pass'; keyframeDistance?: number; hardwareEncoding?: boolean | 'off' | 'auto'; burnCaptions?: boolean; proresProfile?: string; dnxProfile?: string; apvProfile?: string; mxfVideoCodec?: 'dnxhr' | 'proRes' | 'h264'; sdr?: boolean; captionSidecar?: string; loudnessLufs?: number }
export function listFilmcraftExportPresets(token: string, filters: { query?: string; category?: string; format?: string; favorites?: boolean } = {}, signal?: AbortSignal): Promise<{ presets: FilmcraftLibraryPreset[]; default: string }> {
  const query = new URLSearchParams()
  for (const [key, value] of Object.entries(filters)) if (value !== undefined) query.set(key, String(value))
  return requestJson(`/api/compositions/filmcraft/export-presets?${query}`, { headers: bearerHeaders(token), signal })
}
export type FilmcraftPresetCommand =
  | { action: 'get'; name: string }
  | ({ action: 'save'; name: string; from?: string; settings?: Record<string, unknown>; category?: string; description?: string; overwrite?: boolean } & FilmcraftPresetSaveOverrides)
  | { action: 'delete'; name: string }
  | { action: 'favorite'; name: string; favorite?: boolean }
  | { action: 'import'; file: FilmcraftPresetFile }
  | { action: 'export'; names?: string[] }
export function editFilmcraftExportPresets<T>(token: string, command: FilmcraftPresetCommand, signal?: AbortSignal): Promise<T> {
  return requestJson('/api/compositions/filmcraft/export-presets', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify(command), signal })
}
export type FilmcraftScaling = 'scaleToFit' | 'scaleToFill' | 'stretchToFill'
export type FilmcraftExportMetadata = { title?: string; creator?: string; copyright?: string; description?: string; comment?: string }
export type FilmcraftVideoLimiter = { enabled?: boolean; minPercent?: number; maxPercent?: number }
export type FilmcraftOverlayPlacement = 'topLeft' | 'topCenter' | 'topRight' | 'centerLeft' | 'center' | 'centerRight' | 'bottomLeft' | 'bottomCenter' | 'bottomRight'
export type FilmcraftTextOverlay = { enabled: boolean; text: string; placement: FilmcraftOverlayPlacement; offset: [number, number]; sizePercent: number; opacity: number }
export type FilmcraftImageOverlay = Omit<FilmcraftTextOverlay, 'text'> & { sourceId: string }
export type FilmcraftBitrateSettings = { mode: 'cbr' | 'vbr1Pass' | 'vbr2Pass'; targetKbps: number; maxKbps?: number; adaptiveBpp?: number }
export type FilmcraftH264AudioSettings = { codec?: 'auto' | 'aac' | 'pcm'; bitrateKbps?: number; bits?: number; sampleRate?: number; channels?: number }
export type FilmcraftVideoRequest = { mjpegQuality?: number; captionSidecar?: string; h264Level?: number; burnCaptions?: boolean; scale?: number; libraryPreset?: string; document: string; sequenceId: string; startFrames: string; frames: number; sources: Record<string, string>; withAudio?: boolean; preset?: FilmcraftExportPreset; frameSize?: [number, number]; frameRate?: { numerator: number; denominator: number }; keyframeDistance?: number; h264Profile?: 'baseline' | 'main' | 'high'; multiplexer?: 'mp4' | 'mov'; h264Audio?: FilmcraftH264AudioSettings; audio?: FilmcraftH264AudioSettings; bitrate?: FilmcraftBitrateSettings; scaling?: FilmcraftScaling; pixelAspect?: [number, number]; maxRenderQuality?: boolean; metadata?: FilmcraftExportMetadata; videoLimiter?: FilmcraftVideoLimiter; loudness?: { enabled: boolean; targetLufs?: number; truePeakDbtp?: number }; nameOverlay?: FilmcraftTextOverlay; timecodeOverlay?: FilmcraftTextOverlay; imageOverlay?: FilmcraftImageOverlay }
export function startFilmcraftVideoJob(token: string, input: FilmcraftVideoRequest, signal?: AbortSignal): Promise<{ jobId: string; mode: 'video_only' | 'video_audio' | 'audio_only'; container: 'mkv' | 'mp4' | 'mov' | 'mxf' | 'zip' | 'gif' | 'wav' | 'aiff' }> {
  return requestJson('/api/compositions/filmcraft/video-jobs', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}
export type FilmcraftQuickExportRequest = { wait?: boolean; document: string; sequenceId: string; sources: Record<string, string>; preset?: string; settings?: Record<string, unknown>; range?: 'entire' | 'inOut' | 'workArea' | 'custom'; sourceRange?: { startTicks: string; endTicks: string } }
export type FilmcraftQuickExportJob = Awaited<ReturnType<typeof startFilmcraftVideoJob>> & { presetName: string; status?: Job['status']; result?: Job['result'] | null; error?: string | null }
export function startFilmcraftQuickExportJob(token: string, input: FilmcraftQuickExportRequest, signal?: AbortSignal): Promise<FilmcraftQuickExportJob> {
  return requestJson('/api/compositions/filmcraft/quick-export', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}
export type FilmcraftExportCommand = `export.presets.${'list' | 'get' | 'save' | 'delete' | 'favorite' | 'import' | 'export'}` | `export.queue.${'add' | 'list' | 'start' | 'stop' | 'cancel' | 'retry' | 'remove' | 'move' | 'clear'}` | 'export.quick' | 'export.formats'
export interface FilmcraftSourceEditFence { sourceItem?: string | null; sourcePlayhead?: string; document: string; sequenceId: string; clipId: string; token: string | null; itemId?: string; binId?: string; projectSelection?: string[]; timelineSelection?: string[]; sourceBindings?: Record<string, string>; timeFrames?: string; timeTicks?: string | null }
export type FilmcraftEffectRackCommand = 'effects.remove' | 'effects.toggleEnabled' | 'effects.reset'
export type FilmcraftBinEditCommand = 'file.newBin' | 'file.newBinFromSelection' | 'project.moveToBin'
export type FilmcraftSelectionCommand = 'edit.selectAllMatching' | 'project.selectAll' | 'project.deselectAll' | 'project.select'
export type FilmcraftNativeCommand = FilmcraftClipEditCommand | FilmcraftMediaModifyCommand | FilmcraftVideoOptionsCommand | FilmcraftReplaceCommand | 'edit.pasteAttributes' | 'edit.removeAttributes' | FilmcraftSubclipCommand | 'clip.subclipDefaults' | FilmcraftSourcePlacementCommand | FilmcraftNavigationCommand | FilmcraftMarksCommand | 'project.delete' | 'edit.duplicate' | FilmcraftSelectionCommand | 'edit.selectLabelGroup' | FilmcraftLabelCommand | FilmcraftBinEditCommand | 'project.renameBin' | 'clip.rename' | FilmcraftExportCommand | FilmcraftEffectRackCommand | 'effects.apply' | 'masks.track' | 'jobs.list' | 'jobs.cancel' | 'masks.add' | 'masks.remove' | 'masks.set' | 'masks.moveVertex' | 'masks.translate' | 'masks.addVertex' | 'masks.removeVertex' | 'masks.toggleVertexSmooth' | 'masks.list' | 'masks.select' | 'effects.toggleAnimation' | 'effects.addKeyframe' | 'effects.setParam' | 'effects.deleteKeyframe' | 'effects.moveKeyframe' | 'effects.setInterpolation' | 'effects.setKeyframe' | 'project.inspect' | 'sequence.inspect' | 'source.inspect' | 'effects.list' | 'command.list' | 'command.batch'
export function runFilmcraftExportCommand<T = unknown>(token: string, command: FilmcraftNativeCommand, params: Record<string, unknown> = {}, signal?: AbortSignal, deadlineUnixMs?: number): Promise<T> {
  return requestJson('/api/compositions/filmcraft/command', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify({ command, params, ...(deadlineUnixMs !== undefined ? { deadlineUnixMs } : {}) }), signal })
}
export type FilmcraftTrackingJobId = number | string
export interface FilmcraftTrackingJobRef { id: FilmcraftTrackingJobId; instance: string }
export interface FilmcraftTrackingJob extends FilmcraftTrackingJobRef { kind: 'maskTracking'; done: number; total: number; finished: boolean; status: string; result: { error?: string; frames?: number } | null; documentText?: string }
function filmcraftTrackingId(value: unknown): FilmcraftTrackingJobId {
  if (typeof value === 'number' && Number.isSafeInteger(value) && value > 0) return value
  if (typeof value === 'string' && value.length <= 20 && /^[1-9]\d*$/.test(value) && BigInt(value) <= 18446744073709551615n) return value
  throw new Error('Invalid exact tracking job ID')
}
function filmcraftTrackingReference(job: FilmcraftTrackingJobId | FilmcraftTrackingJobRef): { job: FilmcraftTrackingJobId; instance?: string } {
  if (typeof job !== 'object') return { job: filmcraftTrackingId(job) }
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(job.instance)) throw new Error('Invalid tracking job instance')
  return { job: filmcraftTrackingId(job.id), instance: job.instance }
}
function filmcraftTrackingRow(value: unknown, expected: FilmcraftTrackingJobId | FilmcraftTrackingJobRef): FilmcraftTrackingJob {
  if (!value || typeof value !== 'object') throw new Error('Invalid tracking job response')
  const row = value as Record<string, unknown>
  const reference = filmcraftTrackingReference(expected)
  if (String(filmcraftTrackingId(row.id)) !== String(reference.job) || row.kind !== 'maskTracking' || !Number.isSafeInteger(row.done) || !Number.isSafeInteger(row.total) || Number(row.done) < 0 || Number(row.total) < 1 || Number(row.done) > Number(row.total) || typeof row.finished !== 'boolean' || typeof row.status !== 'string' || (row.result !== null && (typeof row.result !== 'object' || Array.isArray(row.result))) || (row.finished && !row.result)) throw new Error('Invalid tracking job response')
  const actualReference = filmcraftTrackingReference({ id: filmcraftTrackingId(row.id), instance: String(row.instance ?? '') })
  if (reference.instance !== undefined && reference.instance !== actualReference.instance) throw new Error('Tracking job instance changed')
  if (row.result && 'error' in row.result && typeof row.result.error !== 'string') throw new Error('Invalid tracking job error')
  return row as unknown as FilmcraftTrackingJob
}
export async function startFilmcraftMaskTrackingJob(token: string, input: { document: string; sequence: string; clip: string; effect: number; mask: number; frames: number; direction: 'forward' | 'backward'; sources: Record<string, string> } & FilmcraftParameterClock): Promise<FilmcraftTrackingJob> {
  if (!Number.isSafeInteger(input.frames) || input.frames < 1 || !Number.isSafeInteger(input.effect) || input.effect < 0 || !Number.isSafeInteger(input.mask) || input.mask < 0) throw new Error('Invalid tracking frame count or mask address')
  const { timeTicks, timeFrames, ...address } = input
  const response = await runFilmcraftExportCommand<{ job: unknown; status: unknown }>(token, 'masks.track', { ...address, wait: false, ...(timeTicks !== undefined ? { time: timeTicks } : { frame: timeFrames }) })
  const id = filmcraftTrackingId(response.job)
  return filmcraftTrackingRow(response.status, id)
}
export async function readFilmcraftTrackingJob(token: string, job: FilmcraftTrackingJobId | FilmcraftTrackingJobRef, snapshot = false, signal?: AbortSignal): Promise<FilmcraftTrackingJob> {
  const reference = filmcraftTrackingReference(job)
  const rows = await runFilmcraftExportCommand<unknown>(token, 'jobs.list', { ...reference, snapshot }, signal)
  if (!Array.isArray(rows) || rows.length !== 1) throw new Error('Tracking job is no longer available')
  const row = filmcraftTrackingRow(rows[0], job)
  if (snapshot && (typeof row.documentText !== 'string' || !row.documentText)) throw new Error('Tracking job returned no exact source snapshot')
  return row
}
export function cancelFilmcraftTrackingJob(token: string, job: FilmcraftTrackingJobId | FilmcraftTrackingJobRef): Promise<null> { return runFilmcraftExportCommand(token, 'jobs.cancel', filmcraftTrackingReference(job)) }
export async function importFilmcraftTrackingSnapshot(job: FilmcraftTrackingJob, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  if (!job.finished) throw new Error('Tracking is still running')
  return importFilmcraftCommandSnapshot(job, signal)
}
export async function editFilmcraftEffectRack(token: string, input: { document: string; sequence: string; clip: string; index: number; command: FilmcraftEffectRackCommand }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const { command, ...params } = input
  const result = await runFilmcraftExportCommand<{ documentText?: unknown }>(token, command, params, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export async function renameFilmcraftClip(token: string, input: { document: string; name: string } & ({ sequence: string; clip: string; item?: never } | { item: string; sequence?: never; clip?: never }), signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const result = await runFilmcraftExportCommand(token, 'clip.rename', input, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export async function renameFilmcraftBin(token: string, input: { document: string; bin: string; name: string }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const result = await runFilmcraftExportCommand(token, 'project.renameBin', input, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export async function editFilmcraftBins(token: string, command: FilmcraftBinEditCommand, input: { document: string; name?: string; parent?: string | null; bin?: string | null; items?: string[] }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const result = await runFilmcraftExportCommand(token, command, input, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export async function editFilmcraftLabels(token: string, command: FilmcraftLabelCommand, input: { document: string; sequence?: string; label?: string; items?: string[] | null; clips?: string[] | null; context?: { activeSequence?: string | null; selection?: string[]; projectSelection?: string[] } }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const result = await runFilmcraftExportCommand(token, command, input, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export type FilmcraftLabelGroupSelection = { clips: string[] } | { items: string[] }
export type FilmcraftSubclipCommand = 'clip.makeSubclip' | 'clip.editSubclip'
export interface FilmcraftSubclipInput { document: string; item?: string; clip?: string; name?: string; start?: string; end?: string; startFrame?: string; endFrame?: string; restrictTrims?: boolean; convertToMaster?: boolean; context: { activeSequence: string | null; sourceItem: string | null; selection: string[]; projectSelection: string[]; stillImageDuration?: number; stillImageUnit?: 'frames' | 'seconds' } }
export interface FilmcraftSubclipDefaults extends FilmcraftSubclipInfo { name: string; startFrame: string; endFrame: string; fps: number; frameRate: { num: number; den: number } }
export interface FilmcraftSubclipResult extends FilmcraftImportResult {
  subclipEdit: { itemId: string; parentId: string; created: boolean; converted: boolean; bindingSourceId: string | null }
  subclipProjectSelection: string[]
}
export async function getFilmcraftSubclipDefaults(token: string, input: FilmcraftSubclipInput & { forEdit?: boolean }, signal?: AbortSignal): Promise<FilmcraftSubclipDefaults> {
  const raw = await runFilmcraftExportCommand<Partial<Record<keyof FilmcraftSubclipDefaults, unknown>>>(token, 'clip.subclipDefaults', { ...input }, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (!raw || typeof raw.name !== 'string' || typeof raw.restrictTrims !== 'boolean' || typeof raw.fps !== 'number' || !Number.isFinite(raw.fps) || raw.fps <= 0 || !raw.frameRate || typeof raw.frameRate !== 'object') throw new Error('Subclip defaults returned invalid data')
  const rate = raw.frameRate as Record<string, unknown>
  if ([rate.num, rate.den].some(n => typeof n !== 'number' || !Number.isSafeInteger(n) || n <= 0 || n > 4294967295)) throw new Error('Subclip defaults returned invalid rate')
  return { name: raw.name, parentId: filmcraftSelectionIds([raw.parentId], 'Subclip defaults')[0], startTicks: filmcraftExactTick(raw.startTicks, 'Subclip defaults'), endTicks: filmcraftExactTick(raw.endTicks, 'Subclip defaults'), startFrame: filmcraftExactTick(raw.startFrame, 'Subclip defaults'), endFrame: filmcraftExactTick(raw.endFrame, 'Subclip defaults'), fps: raw.fps, frameRate: { num: rate.num as number, den: rate.den as number }, restrictTrims: raw.restrictTrims }
}
export async function editFilmcraftSubclip(token: string, command: FilmcraftSubclipCommand, input: FilmcraftSubclipInput, signal?: AbortSignal): Promise<FilmcraftSubclipResult> {
  const raw = await runFilmcraftExportCommand<{ result?: { item?: unknown; converted?: unknown }; subclipEdit?: Record<string, unknown>; context?: { projectSelection?: unknown }; documentText?: unknown }>(token, command, { ...input }, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  const info = raw?.subclipEdit
  if (!info || typeof info.created !== 'boolean' || typeof info.converted !== 'boolean') throw new Error('Subclip edit returned missing provenance')
  const [itemId, parentId] = filmcraftSelectionIds([info.itemId, info.parentId], 'Subclip edit')
  const created = command === 'clip.makeSubclip'; const converted = !created && (input.convertToMaster ?? false)
  const bindingSourceId = info.bindingSourceId === null ? null : filmcraftSelectionIds([info.bindingSourceId], 'Subclip edit')[0]
  const selected = filmcraftSelectionIds(raw.context?.projectSelection, 'Subclip edit')
  if (info.created !== created || info.converted !== converted || raw.result?.item !== itemId || (created ? raw.result?.converted !== undefined || JSON.stringify(selected) !== JSON.stringify([itemId]) : raw.result?.converted !== converted || JSON.stringify(selected) !== JSON.stringify(input.context.projectSelection)) || bindingSourceId !== (converted ? parentId : null)) throw new Error('Subclip edit returned inconsistent provenance or selection')
  const project = await importFilmcraftCommandSnapshot(raw, signal)
  const item = project.projectItems?.find(item => item.id === itemId); const parent = project.projectItems?.find(item => item.id === parentId)
  if (!item || itemId === parentId && (created || converted) || (converted ? item.kind !== 'Media' || parent?.kind !== 'Media' : item.kind !== 'Subclip' || item.subclip?.parentId !== parentId) || created && parent?.kind !== 'Media') throw new Error('Subclip snapshot contradicts returned identities')
  return { ...project, subclipEdit: { itemId, parentId, created, converted, bindingSourceId }, subclipProjectSelection: [...selected] }
}
export type FilmcraftSourcePlacementCommand = 'source.insert' | 'source.overwrite'
export interface FilmcraftSourcePlacementResult extends FilmcraftImportResult {
  sourceEdit: true; insertedClipIds: string[]
  sourceEditContext: { activeSequence: string; sourceItem: string; sourcePlayhead: string; playhead: string; playheadFrames: string; selection: string[] }
}
export async function editFilmcraftSource(token: string, command: FilmcraftSourcePlacementCommand, input: { document: string; context: { activeSequence: string; sourceItem: string; sourcePlayhead?: string; playhead?: string; playheadFrames?: string; targeting?: { videoDest: string | null; audioDest: string | null }; sequencesAsClips?: boolean; rippleSequenceMarkers?: boolean; selectionFollowsPlayhead?: boolean; linkedSelection?: boolean; targetedTrackIds?: string[]; defaultMediaScaling?: string; stillImageDuration?: number; stillImageUnit?: 'frames' | 'seconds' } }, signal?: AbortSignal): Promise<FilmcraftSourcePlacementResult> {
  const raw = await runFilmcraftExportCommand<{ sourceEdit?: unknown; documentText?: unknown; result?: { clips?: unknown }; context?: Record<string, unknown> }>(token, command, input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (raw?.sourceEdit !== true || !raw.context) throw new Error('Source edit returned no accepted snapshot context')
  const clips = filmcraftSelectionIds(raw.result?.clips, 'Source edit')
  const [activeSequence, sourceItem] = filmcraftSelectionIds([raw.context.activeSequence, raw.context.sourceItem], 'Source edit')
  const selection = filmcraftSelectionIds(raw.context.selection, 'Source edit')
  const sourcePlayhead = filmcraftExactTick(raw.context.sourcePlayhead, 'Source edit')
  const playhead = filmcraftExactTick(raw.context.playhead, 'Source edit'); const playheadFrames = filmcraftExactTick(raw.context.playheadFrames, 'Source edit')
  if (!clips.length || new Set(clips).size !== clips.length || activeSequence !== input.context.activeSequence || sourceItem !== input.context.sourceItem || sourcePlayhead !== (input.context.sourcePlayhead ?? '0')) throw new Error('Source edit returned inconsistent context')
  const project = await importFilmcraftCommandSnapshot(raw, signal)
  const sequence = project.sequences.find(sequence => sequence.id === activeSequence)
  if (!sequence || !project.projectItems?.some(item => item.id === sourceItem) || selection.some(id => !clips.includes(id) && !sequence.clips?.some(clip => clip.id === id))) throw new Error('Source edit snapshot contradicts returned identities')
  return { ...project, sourceEdit: true, insertedClipIds: [...clips], sourceEditContext: { activeSequence, sourceItem, sourcePlayhead, playhead, playheadFrames, selection: [...selection] } }
}
function filmcraftExactTick(value: unknown, prefix: string): string {
  if (typeof value !== 'string' || !/^-?(0|[1-9]\d{0,18})$/.test(value) || BigInt(value) < -9223372036854775808n || BigInt(value) > 9223372036854775807n || BigInt(value).toString() !== value) throw new Error(`${prefix} returned invalid exact clock`)
  return value
}
export type FilmcraftNavigationCommand = 'sequence.matchFrame' | 'clip.revealInProject' | 'sequence.revealInProject' | 'source.open' | 'source.setPlayhead' | 'markers.goToIn' | 'markers.goToOut' | 'markers.goToSplitVideoIn' | 'markers.goToSplitVideoOut' | 'markers.goToSplitAudioIn' | 'markers.goToSplitAudioOut'
export interface FilmcraftNavigationContext {
  activeSequence: string | null; sourceItem: string | null; sourcePlayhead: string; playhead: string; playheadFrames: string; selection: string[]; projectSelection: string[]
}
export interface FilmcraftNavigationResult { context: FilmcraftNavigationContext; events: Array<{ type: 'openSource' | 'revealInProject'; item: string }> }
export async function navigateFilmcraft(token: string, command: FilmcraftNavigationCommand, input: { document: string; item?: string; clip?: string; clips?: string[]; target?: 'program' | 'source'; time?: string; frame?: string; seconds?: number; context: { activeSequence: string | null; sourceItem: string | null; sourcePlayhead: string; playhead?: string; playheadFrames?: string; selection: string[]; projectSelection: string[]; targetedTrackIds?: string[]; linkedSelection?: boolean; selectionFollowsPlayhead?: boolean; stillImageDuration?: number; stillImageUnit?: 'seconds' | 'frames' } }, signal?: AbortSignal): Promise<FilmcraftNavigationResult> {
  const raw = await runFilmcraftExportCommand<{ result?: unknown; context?: Partial<Record<keyof FilmcraftNavigationContext, unknown>>; events?: unknown }>(token, command, input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  const exactTick = (value: unknown) => filmcraftExactTick(value, 'Navigation')
  const optionalId = (value: unknown) => value === null ? null : filmcraftSelectionIds([value], 'Navigation')[0]
  const context = raw?.context
  if (!context) throw new Error('Navigation returned missing context')
  const fixed: FilmcraftNavigationContext = { activeSequence: optionalId(context.activeSequence), sourceItem: optionalId(context.sourceItem), sourcePlayhead: exactTick(context.sourcePlayhead), playhead: exactTick(context.playhead), playheadFrames: exactTick(context.playheadFrames), selection: filmcraftSelectionIds(context.selection, 'Navigation'), projectSelection: filmcraftSelectionIds(context.projectSelection, 'Navigation') }
  const sourceOpen = command === 'source.open' || command === 'sequence.matchFrame'
  const reveal = command === 'clip.revealInProject' || command === 'sequence.revealInProject'
  if (fixed.activeSequence !== input.context.activeSequence || !reveal && JSON.stringify(fixed.projectSelection) !== JSON.stringify(input.context.projectSelection) || !sourceOpen && fixed.sourceItem !== input.context.sourceItem) throw new Error('Navigation changed unrelated context')
  const sourceOnly = command === 'sequence.matchFrame' || command.startsWith('source.') || command.startsWith('markers.goToSplit') && input.target === 'source'
  if ((sourceOnly || reveal) && JSON.stringify(fixed.selection) !== JSON.stringify(input.context.selection) || !sourceOnly && fixed.sourcePlayhead !== input.context.sourcePlayhead) throw new Error('Navigation changed unrelated monitor state')
  if (!Array.isArray(raw.events)) throw new Error('Navigation returned invalid events')
  const events: FilmcraftNavigationResult['events'] = raw.events.map(event => {
    const type = reveal ? 'revealInProject' : 'openSource'
    if (!event || typeof event !== 'object' || event.type !== type || !sourceOpen && !reveal) throw new Error('Navigation returned invalid source event')
    const item = filmcraftSelectionIds([event.item], 'Navigation')[0]
    if (item !== (reveal ? fixed.projectSelection[0] : fixed.sourceItem) || reveal && fixed.projectSelection.length !== 1) throw new Error('Navigation source event disagrees with context')
    return { type, item }
  })
  if (events.length !== (sourceOpen || reveal ? 1 : 0)) throw new Error('Navigation returned inconsistent event count')
  if (reveal || command === 'sequence.matchFrame') {
    if (!raw.result || typeof raw.result !== 'object' || Object.keys(raw.result).length !== 1 || !('item' in raw.result) || filmcraftSelectionIds([raw.result.item], 'Navigation')[0] !== events[0]?.item) throw new Error('Navigation returned inconsistent item result')
    if (input.context.playhead !== undefined && fixed.playhead !== input.context.playhead) throw new Error('Navigation changed program clock')
  } else if (command === 'source.open' || command === 'markers.goToIn' || command === 'markers.goToOut') {
    if (raw.result !== null) throw new Error('Navigation returned invalid result')
  } else {
    const source = command === 'source.setPlayhead' || input.target === 'source'
    if (!raw.result || typeof raw.result !== 'object' || Object.keys(raw.result).length !== 1 || !('time' in raw.result) || exactTick(raw.result.time) !== (source ? fixed.sourcePlayhead : fixed.playhead)) throw new Error('Navigation returned inconsistent seek result')
  }
  return { context: fixed, events }
}
export type FilmcraftMarksCommand = 'project.setMarks' | 'markers.markIn' | 'markers.markOut' | 'markers.clearIn' | 'markers.clearOut' | 'markers.clearInOut' | 'markers.markSplitVideoIn' | 'markers.markSplitVideoOut' | 'markers.markSplitAudioIn' | 'markers.markSplitAudioOut'
export interface FilmcraftMarkEditResult extends FilmcraftImportResult { markEdit: true }
export type FilmcraftAttributesCommand = 'edit.pasteAttributes' | 'edit.removeAttributes'
export type FilmcraftClipEditCommand = 'clip.speedDuration' | 'clip.enable' | 'clip.link' | 'clip.group' | 'clip.ungroup' | 'clip.scaleToFrameSize' | 'clip.frameHold' | 'clip.insertFrameHoldSegment'
export interface FilmcraftClipEditResult extends FilmcraftImportResult { clipEdit: true; clipEditSelection?: string[] }
export async function editFilmcraftClips(token: string, command: FilmcraftClipEditCommand, input: { document: string; sequence: string; clips?: string[]; clip?: string; speed?: number; reverse?: boolean; ripple?: boolean; interpolation?: string; time?: string; frame?: string; seconds?: number; timecode?: string; context: { activeSequence: string; selection: string[]; linkedSelection: boolean; playhead?: string; playheadFrames?: string; targetedTrackIds?: string[] } }, signal?: AbortSignal): Promise<FilmcraftClipEditResult> {
  const raw = await runFilmcraftExportCommand<{ clipEdit?: unknown; result?: unknown; context?: { selection?: unknown }; documentText?: unknown }>(token, command, { ...input }, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (raw?.clipEdit !== true) throw new Error('Clip command returned no accepted edit marker')
  const selection = filmcraftSelectionIds(raw.context?.selection, 'Clip command')
  const hold = command === 'clip.frameHold' || command === 'clip.insertFrameHoldSegment'
  if (hold) {
    if (!raw.result || typeof raw.result !== 'object') throw new Error('Clip command returned invalid hold result')
    let ids: string[]
    if (command === 'clip.frameHold') ids = filmcraftSelectionIds('clips' in raw.result ? raw.result.clips : null, 'Clip command')
    else {
      ids = filmcraftSelectionIds(['clip' in raw.result ? raw.result.clip : null], 'Clip command')
      const duration = filmcraftExactTick('duration' in raw.result ? raw.result.duration : null, 'Clip command')
      if (BigInt(duration) <= 0n) throw new Error('Clip command returned invalid hold duration')
    }
    if (JSON.stringify(ids) !== JSON.stringify(selection)) throw new Error('Clip command returned inconsistent hold selection')
  } else {
    if (JSON.stringify(selection) !== JSON.stringify(input.context.selection)) throw new Error('Clip command changed unrelated selection')
    if (command === 'clip.link') { if (!raw.result || typeof raw.result !== 'object' || !('linked' in raw.result) || typeof raw.result.linked !== 'boolean') throw new Error('Clip command returned invalid link result') }
    else if (raw.result !== null) throw new Error('Clip command returned invalid edit result')
  }
  const project = await importFilmcraftCommandSnapshot(raw, signal)
  if (hold && selection.some(id => !project.sequences.find(sequence => sequence.id === input.sequence)?.clips?.some(clip => clip.id === id))) throw new Error('Held clip is absent from imported sequence')
  return { ...project, clipEdit: true, ...(hold ? { clipEditSelection: [...selection] } : {}) }
}
export interface FilmcraftMediaInterpretation { audioChannels: number | null; channelMap: { format: string; clips: number[][] } | null; startTimecode: string | null; tapeName: string | null; fileTimecode: string | null }
export type FilmcraftMediaModifyCommand = 'clip.audioChannels' | 'clip.modifyTimecode' | 'clip.breakoutToMono'
export interface FilmcraftMediaModifyResult extends FilmcraftImportResult { modifyEdit: true; modificationCount: number; modifiedProjectSelection?: string[]; duplicateSources?: { id: string; sourceId: string }[] }
export async function modifyFilmcraftMedia(token: string, command: FilmcraftMediaModifyCommand, input: { document: string; item?: string; items?: string[]; format?: string; clips?: string[] | number[][]; channels?: number[]; frame?: string; timecode?: string; tapeName?: string; reset?: boolean; context: { activeSequence: string | null; selection: string[]; projectSelection: string[] } }, signal?: AbortSignal): Promise<FilmcraftMediaModifyResult> {
  const raw = await runFilmcraftExportCommand<{ modifyEdit?: unknown; result?: Record<string, unknown>; context?: { projectSelection?: unknown }; copies?: unknown; documentText?: unknown }>(token, command, { ...input }, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (raw?.modifyEdit !== true) throw new Error('Media modify returned no accepted edit marker')
  const selected = filmcraftSelectionIds(raw.context?.projectSelection, 'Media modify')
  let count = 0
  let aliases: { id: string; sourceId: string }[] | undefined
  if (command === 'clip.breakoutToMono') {
    const made = filmcraftSelectionIds(raw.result?.items, 'Media modify')
    if (JSON.stringify(made) !== JSON.stringify(selected) || !Array.isArray(raw.copies) || raw.copies.length !== made.length || new Set(made).size !== made.length) throw new Error('Media modify returned inconsistent mono aliases')
    const sources = input.items ?? (input.item ? [input.item] : input.context.projectSelection)
    aliases = raw.copies.map((entry: unknown, index: number) => {
      if (!entry || typeof entry !== 'object' || !('id' in entry) || !('sourceId' in entry)) throw new Error('Media modify returned invalid alias provenance')
      const id = filmcraftSelectionIds([entry.id], 'Media modify')[0], sourceId = filmcraftSelectionIds([entry.sourceId], 'Media modify')[0]
      if (id !== made[index] || !sources.includes(sourceId)) throw new Error('Media modify returned invalid alias provenance')
      return { id, sourceId }
    })
    count = made.length
  } else {
    if (JSON.stringify(selected) !== JSON.stringify(input.context.projectSelection)) throw new Error('Media modify changed project selection')
    if (command === 'clip.modifyTimecode') {
      const id = filmcraftSelectionIds([raw.result?.item], 'Media modify')[0]
      if (input.item !== undefined && id !== input.item) throw new Error('Media modify returned unrelated timecode item')
      if (raw.result?.startTimecode !== null) filmcraftExactTick(raw.result?.startTimecode, 'Media modify')
      count = 1
    } else if (raw.result?.channels !== undefined) {
      count = filmcraftSelectionIds(raw.result.clips, 'Media modify').length
      if (!Array.isArray(raw.result.channels) || raw.result.channels.some(n => typeof n !== 'number' || !Number.isInteger(n) || n < 0 || n > 65535)) throw new Error('Media modify returned invalid channels')
    } else {
      if (!Array.isArray(raw.result?.items)) throw new Error('Media modify returned invalid channel maps')
      for (const entry of raw.result.items) {
        if (!entry || typeof entry !== 'object') throw new Error('Media modify returned invalid channel maps')
        filmcraftSelectionIds([entry.item], 'Media modify')
        if (!['Mono', 'Stereo', 'Surround51', 'Adaptive'].includes(entry.format) || !Array.isArray(entry.clips) || entry.clips.some((row: unknown) => !Array.isArray(row) || row.some(n => typeof n !== 'number' || !Number.isInteger(n) || n < 0 || n > 65535))) throw new Error('Media modify returned invalid channel maps')
      }
      count = raw.result.items.length
    }
  }
  const project = await importFilmcraftCommandSnapshot(raw, signal)
  if (aliases?.some(alias => !project.projectItems?.some(item => item.id === alias.id && item.kind === 'Media'))) throw new Error('Media modify alias is absent from imported project')
  return { ...project, modifyEdit: true, modificationCount: count, ...(aliases ? { modifiedProjectSelection: [...selected], duplicateSources: aliases } : {}) }
}
export type FilmcraftVideoOptionsCommand = 'clip.frameHoldOptions' | 'clip.fieldOptions' | 'clip.timeInterpolation.frameSampling' | 'clip.timeInterpolation.frameBlending' | 'clip.timeInterpolation.opticalFlow' | 'clip.setTimeInterpolation' | 'clip.fitToFrame' | 'clip.fillFrame'
export interface FilmcraftVideoOptionsResult extends FilmcraftImportResult { videoOptionsEdit: true; videoOptionsClipCount: number }
export async function editFilmcraftVideoOptions(token: string, command: FilmcraftVideoOptionsCommand, input: { document: string; sequence: string; clips: string[]; enabled?: boolean; holdOn?: string; holdFilters?: boolean; time?: string; frame?: string; seconds?: number; timecode?: string; processing?: string; reverseFieldDominance?: boolean; mode?: string; context: { activeSequence: string; selection: string[]; playhead?: string; playheadFrames?: string } }, signal?: AbortSignal): Promise<FilmcraftVideoOptionsResult> {
  const raw = await runFilmcraftExportCommand<{ videoOptionsEdit?: unknown; result?: { clips?: unknown; mode?: unknown }; documentText?: unknown }>(token, command, input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (raw?.videoOptionsEdit !== true) throw new Error('Video options returned no accepted edit marker')
  const clips = raw.result?.clips
  let count: number
  if (command === 'clip.frameHoldOptions' || command === 'clip.fitToFrame' || command === 'clip.fillFrame') {
    if (!Array.isArray(clips)) throw new Error('Video options returned invalid clip report')
    for (const clip of clips) {
      if (!clip || typeof clip !== 'object') throw new Error('Video options returned invalid clip report')
      filmcraftSelectionIds([clip.clip], 'Video options')
      if (command === 'clip.frameHoldOptions') { if (BigInt(filmcraftExactTick(clip.hold, 'Video options')) < 0n) throw new Error('Video options returned invalid hold') }
      else if (typeof clip.scale !== 'number' || !Number.isFinite(clip.scale) || clip.scale <= 0) throw new Error('Video options returned invalid scale')
    }
    count = clips.length
  } else {
    if (typeof clips !== 'number' || !Number.isSafeInteger(clips) || clips < 0) throw new Error('Video options returned invalid clip count')
    count = clips
    if (command !== 'clip.fieldOptions' && !['frameSampling', 'frameBlending', 'opticalFlow'].includes(String(raw.result?.mode))) throw new Error('Video options returned invalid interpolation')
  }
  return { ...await importFilmcraftCommandSnapshot(raw, signal), videoOptionsEdit: true, videoOptionsClipCount: count }
}
export type FilmcraftReplaceCommand = 'clip.replaceFromSource' | 'clip.replaceFromSourceMatchFrame' | 'clip.replaceFromBin'
export interface FilmcraftReplaceResult extends FilmcraftImportResult { replacementEdit: true; replacedClipIds: string[]; replacementItemId: string; sourceInClamped: string[]; shortClips: { clip: string; shortByFrames: string }[] }
export async function replaceFilmcraftClips(token: string, command: FilmcraftReplaceCommand, input: { document: string; sequence: string; clips: string[]; item?: string; keepSourceIn?: boolean; context: { activeSequence: string; selection: string[]; projectSelection: string[]; sourceItem: string | null; sourcePlayhead: string; playhead?: string; playheadFrames?: string; linkedSelection: boolean } }, signal?: AbortSignal): Promise<FilmcraftReplaceResult> {
  const raw = await runFilmcraftExportCommand<{ replacementEdit?: unknown; result?: { clips?: unknown; item?: unknown; short?: unknown; sourceInClamped?: unknown }; documentText?: unknown }>(token, command, input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (raw?.replacementEdit !== true) throw new Error('Replacement returned no accepted edit marker')
  const replacedClipIds = filmcraftSelectionIds(raw.result?.clips, 'Replacement')
  if (!replacedClipIds.length) throw new Error('Replacement returned no clips')
  const replacementItemId = filmcraftSelectionIds([raw.result?.item], 'Replacement')[0]
  if (command !== 'clip.replaceFromBin' && replacementItemId !== input.context.sourceItem || command === 'clip.replaceFromBin' && input.item !== undefined && replacementItemId !== input.item) throw new Error('Replacement returned an unrelated item')
  const sourceInClamped = filmcraftSelectionIds(raw.result?.sourceInClamped ?? [], 'Replacement')
  if (sourceInClamped.some(id => !replacedClipIds.includes(id)) || sourceInClamped.length && !(command === 'clip.replaceFromBin' && input.keepSourceIn)) throw new Error('Replacement returned invalid clamp report')
  if (!Array.isArray(raw.result?.short)) throw new Error('Replacement returned invalid short media report')
  const shortClips = raw.result.short.map((entry: unknown) => {
    if (!entry || typeof entry !== 'object' || !('clip' in entry) || !('shortByFrames' in entry)) throw new Error('Replacement returned invalid short media report')
    const clip = filmcraftSelectionIds([entry.clip], 'Replacement')[0]
    const shortByFrames = filmcraftExactTick(entry.shortByFrames, 'Replacement')
    if (!replacedClipIds.includes(clip) || BigInt(shortByFrames) <= 0n) throw new Error('Replacement returned invalid short media report')
    return { clip, shortByFrames }
  })
  return { ...await importFilmcraftCommandSnapshot(raw, signal), replacementEdit: true, replacedClipIds, replacementItemId, sourceInClamped, shortClips }
}
export interface FilmcraftAttributesResult extends FilmcraftImportResult { attributeEdit: true; changedClipCount: number }
export async function editFilmcraftAttributes(token: string, command: FilmcraftAttributesCommand, input: { document: string; sequence: string; clips: string[]; clipboardText?: string; motion: boolean; opacity: boolean; timeRemapping: boolean; volume: boolean; channelVolume: boolean; panner: boolean; effects: boolean | string[]; scaleTimes: boolean; context: { activeSequence: string; selection: string[]; linkedSelection: boolean } }, signal?: AbortSignal): Promise<FilmcraftAttributesResult> {
  const raw = await runFilmcraftExportCommand<{ attributeEdit?: unknown; result?: { clips?: unknown }; documentText?: unknown }>(token, command, input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  const count = raw?.result?.clips
  if (raw?.attributeEdit !== true || typeof count !== 'number' || !Number.isSafeInteger(count) || count < 0) throw new Error('Attributes command returned invalid edit result')
  return { ...await importFilmcraftCommandSnapshot(raw, signal), attributeEdit: true, changedClipCount: count }
}
export async function editFilmcraftMarks(token: string, command: FilmcraftMarksCommand, input: { document: string; item?: string; in?: string | null; out?: string | null; target?: 'program' | 'source'; time?: string; frame?: string; seconds?: number; timecode?: string; context?: { activeSequence?: string | null; sourceItem?: string | null; playhead?: string; sourcePlayhead?: string } }, signal?: AbortSignal): Promise<FilmcraftMarkEditResult> {
  const raw = await runFilmcraftExportCommand<{ markEdit?: unknown; documentText?: unknown }>(token, command, input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  if (raw?.markEdit !== true) throw new Error('Marks command returned no accepted edit marker')
  return { ...await importFilmcraftCommandSnapshot(raw, signal), markEdit: true }
}
export interface FilmcraftDeleteResult extends FilmcraftImportResult {
  deletedItemIds: string[]; deletedBinIds: string[]
  deletionContext: { activeSequence: string | null; selection: string[]; projectSelection: string[] }
}
export async function deleteFilmcraftProjectItems(token: string, input: { document: string; items: string[]; context: { activeSequence: string | null; selection: string[]; projectSelection: string[] } }, signal?: AbortSignal): Promise<FilmcraftDeleteResult> {
  const raw = await runFilmcraftExportCommand<{ result?: { items?: unknown; bins?: unknown }; removedItemIds?: unknown; removedBinIds?: unknown; context?: { activeSequence?: unknown; selection?: unknown; projectSelection?: unknown }; documentText?: unknown }>(token, 'project.delete', input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  const count = (n: unknown) => typeof n === 'number' && Number.isSafeInteger(n) && n >= 0
  if (!count(raw?.result?.items) || !count(raw?.result?.bins)) throw new Error('Delete returned invalid counts')
  const items = filmcraftSelectionIds(raw.removedItemIds, 'Delete')
  const bins = filmcraftSelectionIds(raw.removedBinIds, 'Delete')
  const selection = filmcraftSelectionIds(raw.context?.selection, 'Delete')
  const projectSelection = filmcraftSelectionIds(raw.context?.projectSelection, 'Delete')
  const active = raw.context?.activeSequence === null ? null : filmcraftSelectionIds([raw.context?.activeSequence], 'Delete')[0]
  if (new Set(items).size !== items.length || new Set(bins).size !== bins.length || items.length > (raw.result!.items as number) || projectSelection.length) throw new Error('Delete returned inconsistent metadata')
  const project = await importFilmcraftCommandSnapshot(raw, signal)
  const sequence = project.sequences.find(sequence => sequence.id === active)
  if (active !== null && !sequence || selection.some(id => !sequence?.clips?.some(clip => clip.id === id)) || items.some(id => project.projectItems?.some(item => item.id === id)) || bins.some(id => project.projectBins?.some(bin => bin.id === id))) throw new Error('Delete snapshot contradicts fixed context')
  return { ...project, deletedItemIds: [...items], deletedBinIds: [...bins], deletionContext: { activeSequence: active, selection: [...selection], projectSelection: [] } }
}
export interface FilmcraftDuplicateResult extends FilmcraftImportResult { duplicatedItemIds: string[]; duplicateSources: Array<{ id: string; sourceId: string }> }
export async function duplicateFilmcraftProjectItems(token: string, input: { document: string; context: { projectSelection: string[] } }, signal?: AbortSignal): Promise<FilmcraftDuplicateResult> {
  const raw = await runFilmcraftExportCommand<{ result?: { items?: unknown }; context?: { projectSelection?: unknown }; copies?: unknown; documentText?: unknown }>(token, 'edit.duplicate', input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  const ids = filmcraftSelectionIds(raw?.result?.items, 'Duplicate')
  if (new Set(ids).size !== ids.length || JSON.stringify(ids) !== JSON.stringify(raw?.context?.projectSelection)) throw new Error('Duplicate returned inconsistent selection')
  if (!Array.isArray(raw.copies) || raw.copies.length !== ids.length) throw new Error('Duplicate returned missing copy provenance')
  let position = 0
  const sources = raw.copies.map((record, index) => {
    if (!record || typeof record !== 'object') throw new Error('Duplicate returned invalid copy provenance')
    const [id, sourceId] = filmcraftSelectionIds([record.id, record.sourceId], 'Duplicate')
    const sourcePosition = input.context.projectSelection.indexOf(sourceId, position)
    if (id !== ids[index] || id === sourceId || sourcePosition < 0) throw new Error('Duplicate returned inconsistent copy provenance')
    position = sourcePosition + 1
    return { id, sourceId }
  })
  const project = await importFilmcraftCommandSnapshot(raw, signal)
  const known = new Set(project.projectItems?.map(item => item.id) ?? [])
  if (ids.some(id => !known.has(id)) || sources.some(record => !known.has(record.sourceId))) throw new Error('Duplicate snapshot omitted copied items')
  return { ...project, duplicatedItemIds: [...ids], duplicateSources: sources }
}
function filmcraftSelectionIds(ids: unknown, prefix: string): string[] {
  if (!Array.isArray(ids) || ids.some(id => typeof id !== 'string' || !/^(0|[1-9]\d{0,19})$/.test(id) || BigInt(id) > 18446744073709551615n)) throw new Error(`${prefix} returned invalid source IDs`)
  return ids
}
export async function selectFilmcraftSelection(token: string, command: FilmcraftSelectionCommand, input: { document: string; sequence?: string; clips?: string[] | null; clip?: string; items?: string[] | null; context: { selection: string[]; projectSelection: string[] } }, signal?: AbortSignal): Promise<FilmcraftLabelGroupSelection> {
  const raw = await runFilmcraftExportCommand<{ result?: unknown; context?: unknown }>(token, command, input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  const context = raw?.context
  if (!context || typeof context !== 'object' || Array.isArray(context)) throw new Error('Selection command returned invalid context')
  const clips = filmcraftSelectionIds('selection' in context ? context.selection : null, 'Selection command')
  const items = filmcraftSelectionIds('projectSelection' in context ? context.projectSelection : null, 'Selection command')
  if (command === 'edit.selectAllMatching') {
    if (JSON.stringify(items) !== JSON.stringify(input.context.projectSelection)) throw new Error('Selection command changed project selection')
    if (!raw.result || typeof raw.result !== 'object' || Object.keys(raw.result).length !== 1 || !('clips' in raw.result) || JSON.stringify(raw.result.clips) !== JSON.stringify(clips)) throw new Error('Selection command returned mismatched clips')
    return { clips }
  }
  if (JSON.stringify(clips) !== JSON.stringify(input.context.selection)) throw new Error('Selection command changed timeline selection')
  if (command === 'project.selectAll') {
    if (!raw.result || typeof raw.result !== 'object' || Object.keys(raw.result).length !== 1 || !('selected' in raw.result) || raw.result.selected !== items.length || new Set(items).size !== items.length) throw new Error('Selection command returned invalid count')
  } else {
    if (raw.result !== null || (command === 'project.deselectAll' ? items.length !== 0 : JSON.stringify(items) !== JSON.stringify(input.items ?? []))) throw new Error('Selection command returned mismatched project selection')
  }
  return { items }
}
export async function selectFilmcraftLabelGroup(token: string, input: { document: string; sequence?: string; context: { selection: string[]; projectSelection: string[] } }, signal?: AbortSignal): Promise<FilmcraftLabelGroupSelection> {
  const raw = await runFilmcraftExportCommand<{ result?: unknown }>(token, 'edit.selectLabelGroup', input, signal)
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  const result = raw?.result
  if (!result || typeof result !== 'object' || Array.isArray(result) || Object.keys(result).length !== 1) throw new Error('Label group returned an invalid selection')
  const key = 'clips' in result ? 'clips' : 'items' in result ? 'items' : null
  if (key && key !== (input.context.selection.length ? 'clips' : 'items')) throw new Error('Label group returned invalid selection scope')
  const ids: unknown = key ? (result as Record<string, unknown>)[key] : null
  const exactIds = filmcraftSelectionIds(ids, 'Label group')
  return key === 'clips' ? { clips: exactIds } : { items: exactIds }
}
function importFilmcraftCommandSnapshot(result: unknown, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  if (!result || typeof result !== 'object' || !('documentText' in result) || typeof result.documentText !== 'string' || !result.documentText) throw new Error('Effect command returned no source snapshot')
  if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
  return importFilmcraftProject(result.documentText, signal)
}
export type FilmcraftEffectKind = 'Video' | 'Audio' | 'VideoTransition' | 'AudioTransition'
export interface FilmcraftEffectOption { id: string; name: string; kind: FilmcraftEffectKind; category: string[]; folder: string | null; path: string; params: string[]; badges: { accelerated: boolean; float32: boolean; yuv: boolean } }
export function listFilmcraftEffects(token: string, kind: FilmcraftEffectKind, signal?: AbortSignal): Promise<FilmcraftEffectOption[]> {
  return runFilmcraftExportCommand(token, 'effects.list', { kind }, signal)
}
export async function editFilmcraftAnimation(token: string, input: { command: 'effects.toggleAnimation' | 'effects.addKeyframe'; document: string; sequence: string; clip: string; effect: number; param: string; mask?: number; playhead: string }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const { command, playhead, ...address } = input
  const result = await runFilmcraftExportCommand(token, command, { ...address, context: { playhead } }, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export async function setFilmcraftEffectParameter(token: string, input: { document: string; sequence: string; clip: string; effect: number; param: string; value: FilmcraftParameterValue; mask?: number } & FilmcraftParameterClock, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const { value, timeTicks, timeFrames, ...address } = input
  let raw: unknown
  if ('Vec2' in value) {
    if (value.Vec2.x === null || value.Vec2.y === null) throw new Error('Для применения в позиции курсора задайте численные координаты вместо auto')
    raw = [value.Vec2.x, value.Vec2.y]
  } else raw = Object.values(value)[0]
  const result = await runFilmcraftExportCommand(token, 'effects.setParam', { ...address, value: raw, ...(timeTicks !== undefined ? { time: timeTicks } : { frame: timeFrames }) }, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export async function applyFilmcraftEffect(token: string, input: { document: string; sequence: string; clip: string; effect: string }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  const result = await runFilmcraftExportCommand(token, 'effects.apply', input, signal)
  return importFilmcraftCommandSnapshot(result, signal)
}
export interface FilmcraftExportFormat {
  id: string; label: string; extension: string; video: boolean; audio: boolean; imageSequence: boolean
  available: boolean; native: boolean; hostExtension: string | null; unavailableReason: string | null; availabilityScope: string
}
export function listFilmcraftExportFormats(token: string, signal?: AbortSignal): Promise<FilmcraftExportFormat[]> {
  return runFilmcraftExportCommand(token, 'export.formats', {}, signal)
}
export type FilmcraftSourceRange = { startTicks: string; endTicks: string }
export interface FilmcraftExportQueueItem {
  id: string; sequenceId: string; sequenceName: string; preset: string; container: string
  status: 'ready' | 'encoding' | 'done' | 'failed' | 'cancelled'
  jobId: string | null; error: string | null; progress: number | null
  statusText: string | null; etaSeconds: number | null; etaText?: string | null; sourceRange: FilmcraftSourceRange | null
}
export interface FilmcraftExportQueue { running: boolean; items: FilmcraftExportQueueItem[] }
export type FilmcraftExportQueueCommand =
  | { action: 'add'; input: Omit<FilmcraftVideoRequest, 'frames' | 'startFrames'> & { frames?: number; startFrames?: string; sourceRange?: FilmcraftSourceRange }; sequences?: string[]; ranges?: Array<'entire' | 'inOut' | 'workArea' | FilmcraftSourceRange>; start?: boolean; wait?: boolean }
  | { action: 'start'; wait?: boolean }
  | { action: 'stop' }
  | { action: 'cancel'; id?: string; wait?: boolean }
  | { action: 'retry'; id: string; start?: boolean; wait?: boolean }
  | { action: 'remove'; id: string }
  | { action: 'move'; id: string; to?: number; by?: number }
  | { action: 'clear'; all?: boolean }
export function listFilmcraftExportQueue(token: string, signal?: AbortSignal): Promise<FilmcraftExportQueue> {
  return requestJson('/api/compositions/filmcraft/export-queue', { headers: bearerHeaders(token), signal })
}
export function editFilmcraftExportQueue(token: string, command: FilmcraftExportQueueCommand, signal?: AbortSignal): Promise<FilmcraftExportQueue> {
  return requestJson('/api/compositions/filmcraft/export-queue', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'application/json' }, body: JSON.stringify(command), signal })
}
export interface FilmcraftVideoHistoryEntry { jobId: string; createdAt: number; job: Job }
export interface FilmcraftVideoHistory { jobs: FilmcraftVideoHistoryEntry[]; nextCursor: string | null }
export function listFilmcraftVideoJobs(token: string, cursor?: string, signal?: AbortSignal): Promise<FilmcraftVideoHistory> {
  const query = new URLSearchParams({ limit: '20' }); if (cursor) query.set('cursor', cursor)
  return requestJson(`/api/compositions/filmcraft/video-jobs?${query}`, { headers: bearerHeaders(token), signal })
}
export function getFilmcraftVideoJob(token: string, id: string, signal?: AbortSignal): Promise<Job> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}`, { headers: bearerHeaders(token), signal })
}
export function retryFilmcraftVideoJob(token: string, id: string, signal?: AbortSignal): Promise<{ jobId: string; status: 'pending' }> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}/retry`, { method: 'POST', headers: bearerHeaders(token), signal })
}
export function cancelFilmcraftVideoJob(token: string, id: string, signal?: AbortSignal): Promise<unknown> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}/cancel`, { method: 'POST', headers: bearerHeaders(token), signal })
}
export async function downloadFilmcraftVideo(token: string, id: string, signal?: AbortSignal, format: 'mkv' | 'mp4' | 'mov' | 'mxf' | 'zip' | 'gif' | 'wav' | 'aiff' = 'mkv'): Promise<Blob> {
  if (format !== 'mkv' && format !== 'mp4' && format !== 'mov' && format !== 'mxf' && format !== 'zip' && format !== 'gif' && format !== 'wav' && format !== 'aiff') throw new Error('Неподдерживаемый формат видео')
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)) throw new Error('Некорректный идентификатор видео')
  const response = await safeFetch(`/files/outputs/${id}.${format}`, { headers: bearerHeaders(token), signal })
  await requireOk(response, 'Не удалось скачать видео')
  return response.blob()
}
export async function exportFilmcraftFrame(input: FilmcraftPreviewInput, token?: string | null, signal?: AbortSignal): Promise<Blob> {
  const response = await safeFetch('/api/compositions/filmcraft/export-frame', { method: 'POST', headers: { ...(token ? bearerHeaders(token) : {}), 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
  await requireOk(response, 'Не удалось экспортировать кадр проекта')
  return response.blob()
}
export function getFilmcraftPreviewFrame(input: FilmcraftPreviewInput, signal?: AbortSignal, token?: string | null): Promise<FilmcraftPreviewFrame> {
  return requestJson('/api/compositions/filmcraft/preview-frame', { method: 'POST', headers: { ...(token ? bearerHeaders(token) : {}), 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export type ConcatSourceEditCommand =
  | {kind:'move';startTick:number}
  | {kind:'setCurve';points:import('./composition/types').ConcatSourceDocumentClock['speedCurve']}
  | {kind:'split';time:number}
  | {kind:'trimStart';delta:number;floor:number;ripple:boolean}
  | {kind:'trimEnd';delta:number;ceiling:number|null;ripple:boolean}
  | {kind:'freeze';time:number;duration:number|null}
export interface ConcatSourceEditTracks {
  clip?:Readonly<Record<string,readonly import('./composition/types').WireConcatParamKey[]>>
  audio?:Readonly<Record<string,readonly import('./composition/types').WireConcatParamKey[]>>
  video?:Readonly<Record<string,readonly import('./composition/types').WireConcatParamKey[]>>
}
export interface ConcatSourceEditPlan {
  capture?:{selection:'atOrAfterThenFirst';sourceSeconds:number;seekTimeUs:number;cacheTimeMs:number;width:number;jpegQuality:number}
  changed:boolean
  rippleBy:number
  parts:readonly {role:'clip'|'head'|'tail'|'still';tracks:ConcatSourceEditTracks;fadeIn:number;fadeOut:number;clock:import('./composition/types').ConcatSourceDocumentClock;placement:import('./composition/types').WireClipPlacement}[]
}
export function planConcatSourceEdit(input:{tracks?:ConcatSourceEditTracks;fadeIn?:number;fadeOut?:number;clock:import('./composition/types').ConcatSourceDocumentClock;rateNum:number;rateDen:number;command:ConcatSourceEditCommand},signal?:AbortSignal):Promise<ConcatSourceEditPlan>{
  return requestJson('/api/compositions/concat-retime-edit',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(input),signal})
}

export interface NativeClipMove {
  fromComposition: string; fromTrackId: string; clipId: string;
  toComposition: string; toTrackId: string; startUs: number; createTrack?: boolean
}
export interface NativeClipMoveResult { project: NativeNestedProject; changed: boolean; affectedCompositions: readonly string[]; movedClipIds: readonly string[] }
export function moveNativeNestedClips(project: NativeNestedProject, moves: readonly NativeClipMove[]): Promise<NativeClipMoveResult> {
  return requestJson('/api/compositions/nested-move-clips', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project, moves }) })
}

export interface LocalSpeechStatus {engine:string;available:boolean;model:string|null;maxDurationUs:number}
export interface LocalSpeechTranscript {segmentation?:'phrase'|'word';language:string;modelHash:string;sourceStartUs:number;durationUs:number;segments:readonly {startUs:number;endUs:number;text:string}[]}
export function getLocalSpeechStatus(token?:string):Promise<LocalSpeechStatus>{return requestJson('/api/speech/models',{headers:token?bearerHeaders(token):undefined})}
export interface LocalSpeechJobUpdate {jobId:string;status:Job['status'];progress?:number}
export async function transcribeLocalSpeech(token:string,input:{sourceId:string;startUs:number;endUs:number;language:string;segmentation?:'phrase'|'word'},signal?:AbortSignal,onJob?:(update:LocalSpeechJobUpdate)=>void):Promise<LocalSpeechTranscript>{
  signal?.throwIfAborted()
  // Keep admission observable even if the panel closes while POST is in flight.
  const {jobId}=await requestJson<{jobId:string}>('/api/speech/jobs',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify(input)})
  if(typeof jobId!=='string'||!jobId)throw new Error('Сервер не вернул номер задания речи')
  return observeLocalSpeechJob(token,jobId,signal,onJob)
}

export interface LocalSpeechJobSummary {jobId:string;sourceId:string;status:Job['status'];progress:number|null;createdAt:number}
export interface LocalSpeechJobPage {jobs:LocalSpeechJobSummary[];nextCursor:string|null}
export function listLocalSpeechJobs(token:string,cursor?:string):Promise<LocalSpeechJobPage>{
  return requestJson(`/api/speech/jobs${cursor?`?cursor=${encodeURIComponent(cursor)}`:''}`,{headers:bearerHeaders(token)})
}

export function cancelLocalSpeechJob(token:string,jobId:string):Promise<unknown>{
  return requestJson(`/api/jobs/${encodeURIComponent(jobId)}/cancel`,{method:'POST',headers:bearerHeaders(token)})
}

/** Reconnect to existing work without admitting a second recognition job. */
export async function observeLocalSpeechJob(token:string,jobId:string,signal?:AbortSignal,onJob?:(update:LocalSpeechJobUpdate)=>void):Promise<LocalSpeechTranscript>{
  if(!jobId||jobId.length>128)throw new Error('Некорректный номер задания речи')
  try{
    onJob?.({jobId,status:'pending'})
    const deadline=Date.now()+8*60*1000
    while(true){
      signal?.throwIfAborted()
      const job=await requestJson<Job>(`/api/jobs/${encodeURIComponent(jobId)}`,{headers:bearerHeaders(token),signal})
      signal?.throwIfAborted()
      onJob?.({jobId,status:job.status,progress:typeof job.progress==='number'&&Number.isFinite(job.progress)?Math.max(0,Math.min(100,job.progress)):undefined})
      if(job.status==='done')return job.result as unknown as LocalSpeechTranscript
      if(job.status==='cancelled')throw new Error('Распознавание отменено')
      if(job.status==='error'||job.status==='interrupted')throw new Error(job.error||'Распознавание остановлено')
      if(Date.now()>=deadline)throw new Error('Превышено время ожидания распознавания')
      await new Promise<void>((resolve,reject)=>{
        const abort=()=>{clearTimeout(timer);signal?.removeEventListener('abort',abort);reject(signal?.reason??new DOMException('Aborted','AbortError'))}
        const timer=setTimeout(()=>{signal?.removeEventListener('abort',abort);resolve()},300)
        signal?.addEventListener('abort',abort,{once:true})
        if(signal?.aborted)abort()
      })
    }
  }catch(reason){
    // Observation failures leave durable work intact. Only explicit abort requests cancellation.
    if(signal?.aborted){
      await cancelLocalSpeechJob(token,jobId)
        .catch(error=>{throw new Error(`Не удалось подтвердить отмену: ${String(error)}`)})
      throw new Error('Отмена распознавания запрошена')
    }
    throw reason
  }
}

export interface SpeechClipPlan {sourceId:string;sourceStartUs:number;sourceEndUs:number;timelineStartUs:number;timelineEndUs:number}
export function planSpeechClip(project:NativeNestedProject,compositionId:string,clipId:string):Promise<SpeechClipPlan>{return requestJson('/api/speech/clip-plan',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,compositionId,clipId})})}
export function alignSpeechClip(project:NativeNestedProject,compositionId:string,clipId:string,sourceId:string,transcript:LocalSpeechTranscript):Promise<import('./composition/types').CaptionTrack>{return requestJson('/api/speech/align',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,compositionId,clipId,sourceId,transcript})})}

export interface FilmcraftAnalysisJobSummary { jobId: string; status: Job['status']; progress: number | null; createdAt: number }
export interface FilmcraftAnalysisJobPage { jobs: FilmcraftAnalysisJobSummary[]; nextCursor: string | null }
export function startFilmcraftAnalysis(token: string, input: FilmcraftPreviewInput, signal?: AbortSignal): Promise<{ jobId: string }> {
  return requestJson('/api/compositions/filmcraft/analysis-jobs', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify(input), signal })
}
export function listFilmcraftAnalysis(token: string, cursor?: string, signal?: AbortSignal): Promise<FilmcraftAnalysisJobPage> {
  return requestJson(`/api/compositions/filmcraft/analysis-jobs${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`, { headers: bearerHeaders(token), signal })
}
export function getFilmcraftAnalysis(token: string, jobId: string, signal?: AbortSignal): Promise<Job> {
  return requestJson(`/api/jobs/${encodeURIComponent(jobId)}`, { headers: bearerHeaders(token), signal })
}
export function cancelFilmcraftAnalysis(token: string, jobId: string, signal?: AbortSignal): Promise<unknown> {
  return requestJson(`/api/jobs/${encodeURIComponent(jobId)}/cancel`, { method: 'POST', headers: bearerHeaders(token), signal })
}

export interface LocalSpeechModel { id: string; name: string; bytes: number; available: boolean; inUse?: boolean }
export interface LocalSpeechModelLibrary { models: LocalSpeechModel[]; configured: LocalSpeechModel | null; selectedId: string; maxBytes: number }
export function listLocalSpeechModels(token: string, signal?: AbortSignal): Promise<LocalSpeechModelLibrary> {
  return requestJson('/api/speech/model-library', { headers: bearerHeaders(token), signal })
}
export function selectLocalSpeechModel(token: string, modelId: string, signal?: AbortSignal): Promise<{ selectedId: string }> {
  return requestJson('/api/speech/model-selection', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ modelId }), signal })
}
export function importLocalSpeechModel(token: string, file: File, signal?: AbortSignal): Promise<LocalSpeechModel> {
  return requestJson(`/api/speech/model-library?${new URLSearchParams({ name: file.name })}`, { method: 'PUT', headers: { ...bearerHeaders(token), 'Content-Type': 'application/octet-stream' }, body: file, signal })
}
export function removeLocalSpeechModel(token: string, modelId: string, signal?: AbortSignal): Promise<{ removed: boolean; fileReleased: boolean }> {
  return requestJson('/api/speech/model-removal', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ modelId }), signal })
}

export interface SpeechCatalogModel { id: string; file: string; bytes: number; sha256: string }
export interface SpeechModelCatalog { revision: string; models: SpeechCatalogModel[] }
export interface SpeechModelDownloadSummary { jobId: string; catalogId: string; createdAt: number; status: Job['status']; progress: number | null }
export function getSpeechModelCatalog(signal?: AbortSignal): Promise<SpeechModelCatalog> {
  return requestJson('/api/speech/model-catalog', { signal })
}
export function startSpeechModelDownload(token: string, catalogId: string, signal?: AbortSignal): Promise<{ jobId: string }> {
  return requestJson('/api/speech/model-downloads', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ catalogId }), signal })
}
export function listSpeechModelDownloads(token: string, cursor?: string, signal?: AbortSignal): Promise<{ jobs: SpeechModelDownloadSummary[]; nextCursor: string | null }> {
  return requestJson(`/api/speech/model-downloads${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`, { headers: bearerHeaders(token), signal })
}
export function getSpeechModelDownload(token: string, id: string, signal?: AbortSignal): Promise<Job> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}`, { headers: bearerHeaders(token), signal })
}
export function cancelSpeechModelDownload(token: string, id: string, signal?: AbortSignal): Promise<unknown> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}/cancel`, { method: 'POST', headers: bearerHeaders(token), signal })
}
export function retrySpeechModelDownload(token: string, id: string, signal?: AbortSignal): Promise<{ jobId: string; status: string }> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}/retry`, { method: 'POST', headers: bearerHeaders(token), signal })
}

export interface LocalTtsVoice { id: number; name: string; language: string }
export interface LocalTtsStatus { engine: string; available: boolean; voices: LocalTtsVoice[]; maxTextBytes: number; minSpeed: number; maxSpeed: number; supportsPauses?: boolean }
export function getLocalTtsStatus(signal?: AbortSignal, token?: string): Promise<LocalTtsStatus> {
  return requestJson('/api/speech/voices', { signal, ...(token ? { headers: bearerHeaders(token) } : {}) })
}
export async function synthesizeLocalSpeech(token: string, input: {text: string; voiceId: number; speed: number}, signal?: AbortSignal): Promise<Blob> {
  const response = await safeFetch('/api/speech/synthesize', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify(input), signal })
  await requireOk(response, 'Не удалось создать озвучку')
  if (!response.headers.get('content-type')?.startsWith('audio/wav')) throw new Error('Сервер не вернул звуковой файл WAV')
  const audio = await response.blob()
  if (audio.size < 44 || audio.size > 64 * 1024 * 1024) throw new Error('Недопустимый размер озвучки')
  return audio
}
export function saveLocalNarration(token: string, projectId: string, audio: Blob, signal?: AbortSignal): Promise<MediaEntry> {
  return requestJson(`/api/speech/narration-assets?${new URLSearchParams({ projectId })}`, { method: 'PUT', headers: { ...bearerHeaders(token), 'Content-Type': 'audio/wav' }, body: audio, signal })
}

export interface LocalTtsJobSummary { jobId: string; voiceId: number; createdAt: number; status: Job['status']; progress: number | null }
export function startLocalTtsJob(token: string, input: {text: string; voiceId: number; speed: number; referenceId?: string; numSteps?: number; pauses?: number}, signal?: AbortSignal): Promise<{ jobId: string }> {
  return requestJson('/api/speech/tts-jobs', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify(input), signal })
}
export function listLocalTtsJobs(token: string, cursor?: string, signal?: AbortSignal): Promise<{ jobs: LocalTtsJobSummary[]; nextCursor: string | null }> {
  return requestJson(`/api/speech/tts-jobs${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`, { headers: bearerHeaders(token), signal })
}
export function getLocalTtsJob(token: string, id: string, signal?: AbortSignal): Promise<Job> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}`, { headers: bearerHeaders(token), signal })
}
export function cancelLocalTtsJob(token: string, id: string, signal?: AbortSignal): Promise<unknown> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}/cancel`, { method: 'POST', headers: bearerHeaders(token), signal })
}
export async function getLocalTtsJobAudio(token: string, id: string, signal?: AbortSignal): Promise<Blob> {
  const response = await safeFetch(`/api/speech/tts-jobs/${encodeURIComponent(id)}/audio`, { headers: bearerHeaders(token), signal })
  await requireOk(response, 'Не удалось загрузить готовую озвучку')
  if (!response.headers.get('content-type')?.startsWith('audio/wav')) throw new Error('Сервер не вернул WAV')
  const blob = await response.blob()
  if (blob.size < 44 || blob.size > 64 * 1024 * 1024) throw new Error('Недопустимый размер озвучки')
  return blob
}

export interface LocalTtsModel { id: string; name: string; bytes: number }
export interface LocalTtsModelLibrary { selectedId: string; models: LocalTtsModel[]; catalog: (LocalTtsModel & { revision: string })[] }
export function listLocalTtsModels(token: string, signal?: AbortSignal): Promise<LocalTtsModelLibrary> {
  return requestJson('/api/speech/tts-models', { headers: bearerHeaders(token), signal })
}
export function selectLocalTtsModel(token: string, modelId: string, signal?: AbortSignal): Promise<{ selectedId: string }> {
  return requestJson('/api/speech/tts-models/selection', { method: 'PUT', headers: bearerHeaders(token, true), body: JSON.stringify({ modelId }), signal })
}
export function startTtsModelDownload(token: string, catalogId: string, signal?: AbortSignal): Promise<{ jobId: string }> {
  return requestJson('/api/speech/tts-model-downloads', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ catalogId }), signal })
}
export function listTtsModelDownloads(token: string, cursor?: string, signal?: AbortSignal): Promise<{ jobs: SpeechModelDownloadSummary[]; nextCursor: string | null }> {
  return requestJson(`/api/speech/tts-model-downloads${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`, { headers: bearerHeaders(token), signal })
}
export function removeLocalTtsModel(token: string, modelId: string, signal?: AbortSignal): Promise<{ removed: boolean; fileReleased: boolean }> {
  return requestJson('/api/speech/tts-models', { method: 'DELETE', headers: bearerHeaders(token, true), body: JSON.stringify({ modelId }), signal })
}

export interface LocalTtsReference { id: string; durationUs: number; sampleRate: number }
export function uploadLocalTtsReference(token: string, audio: Blob, signal?: AbortSignal): Promise<LocalTtsReference> {
  return requestJson('/api/speech/tts-references', { method: 'POST', headers: { ...bearerHeaders(token), 'Content-Type': 'audio/wav' }, body: audio, signal })
}
export interface VoiceReferenceClipPlan { sourceId: string; sourceStartUs: number }
export function planVoiceReferenceClip(project: NativeNestedProject, compositionId: string, clipId: string, signal?: AbortSignal): Promise<VoiceReferenceClipPlan> {
  return requestJson('/api/speech/reference-clip-plan', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project, compositionId, clipId }), signal })
}
export function createLocalTtsReferenceFromSource(token: string, sourceId: string, startUs: number, endUs: number, signal?: AbortSignal): Promise<LocalTtsReference> {
  return requestJson('/api/speech/tts-references/source', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({ sourceId, startUs, endUs }), signal })
}

export interface SavedTtsReference extends LocalTtsReference { origin: 'file'|'clip'; available: boolean }
export interface TtsReferencePage { references: SavedTtsReference[]; nextCursor: string|null }
export function listLocalTtsReferences(token:string,cursor?:string,signal?:AbortSignal):Promise<TtsReferencePage>{
 return requestJson('/api/speech/tts-references'+(cursor?'?cursor='+encodeURIComponent(cursor):''),{headers:bearerHeaders(token),signal})
}
export function removeLocalTtsReference(token:string,referenceId:string,signal?:AbortSignal):Promise<{removed:boolean;filesRetained:boolean}>{
 return requestJson('/api/speech/tts-references',{method:'DELETE',headers:bearerHeaders(token,true),body:JSON.stringify({referenceId}),signal})
}

export interface ImageEnhancementStatus { available: boolean; engine: string; factors: number[]; stillImages: boolean; video: boolean }
export function getImageEnhancementStatus(token: string, signal?: AbortSignal): Promise<ImageEnhancementStatus> {
  return requestJson('/api/enhance/status', { headers: bearerHeaders(token), signal })
}
export function startImageEnhancement(token: string, sourceId: string, factor: number, signal?: AbortSignal): Promise<{jobId: string}> {
  return requestJson('/api/enhance/jobs', { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({sourceId, factor}), signal })
}
export function getImageEnhancementJob(token: string, id: string, signal?: AbortSignal): Promise<Job> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}`, { headers: bearerHeaders(token), signal })
}
export function cancelImageEnhancement(token: string, id: string, signal?: AbortSignal): Promise<unknown> {
  return requestJson(`/api/jobs/${encodeURIComponent(id)}/cancel`, { method: 'POST', headers: bearerHeaders(token), signal })
}
export async function getEnhancedImage(token: string, id: string, signal?: AbortSignal): Promise<Blob> {
  const response = await safeFetch(`/api/enhance/jobs/${encodeURIComponent(id)}/image`, { headers: bearerHeaders(token), signal })
  await requireOk(response, 'Не удалось загрузить улучшенное изображение')
  if (!response.headers.get('content-type')?.startsWith('image/png')) throw new Error('Сервер не вернул PNG')
  const blob = await response.blob()
  if (blob.size < 8 || blob.size > 256 * 1024 * 1024) throw new Error('Недопустимый размер изображения')
  return blob
}
export function saveEnhancedImage(token: string, id: string, projectId: string, signal?: AbortSignal): Promise<MediaEntry> {
  return requestJson(`/api/enhance/jobs/${encodeURIComponent(id)}/asset`, { method: 'POST', headers: bearerHeaders(token, true), body: JSON.stringify({projectId}), signal })
}

export interface ImageEnhancementSummary { jobId: string; sourceId: string; factor: number | null; createdAt: number; status: Job['status']; progress: number | null }
export function listImageEnhancements(token: string, sourceId?: string, cursor?: string, signal?: AbortSignal): Promise<{jobs: ImageEnhancementSummary[]; nextCursor: string | null}> {
  const query = new URLSearchParams()
  if (sourceId) query.set('sourceId', sourceId)
  if (cursor) query.set('cursor', cursor)
  return requestJson(`/api/enhance/jobs?${query}`, { headers: bearerHeaders(token), signal })
}

export interface EnhancementModel { id:string; name:string; bytes:number; revision?:string; licence?:string }
export interface EnhancementModelLibrary { models:EnhancementModel[]; catalog:EnhancementModel[]; selectedId:string }
export function listEnhancementModels(token:string,signal?:AbortSignal):Promise<EnhancementModelLibrary>{
 return requestJson('/api/enhance/models',{headers:bearerHeaders(token),signal})
}
export function selectEnhancementModel(token:string,modelId:string,signal?:AbortSignal):Promise<{selectedId:string}>{
 return requestJson('/api/enhance/models/select',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({modelId}),signal})
}
export function removeEnhancementModel(token:string,modelId:string,signal?:AbortSignal):Promise<{removed:boolean;fileReleased:boolean}>{
 return requestJson('/api/enhance/models/remove',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({modelId}),signal})
}
export function startEnhancementModelDownload(token:string,catalogId:string,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/enhance/models/downloads',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({catalogId}),signal})
}
export function listEnhancementModelDownloads(token:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:SpeechModelDownloadSummary[];nextCursor:string|null}>{
 return requestJson('/api/enhance/models/downloads'+(cursor?'?cursor='+encodeURIComponent(cursor):''),{headers:bearerHeaders(token),signal})
}

export function startVideoEnhancement(token:string,sourceId:string,factor:number,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/enhance/video-jobs',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({sourceId,factor}),signal})
}
export function saveEnhancedVideo(token:string,id:string,projectId:string,signal?:AbortSignal):Promise<MediaEntry>{
 return requestJson('/api/enhance/video-jobs/'+encodeURIComponent(id)+'/asset',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({projectId}),signal})
}
export function listVideoEnhancements(token:string,sourceId?:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:ImageEnhancementSummary[];nextCursor:string|null}>{
 const query=new URLSearchParams();if(sourceId)query.set('sourceId',sourceId);if(cursor)query.set('cursor',cursor)
 return requestJson('/api/enhance/video-jobs?'+query,{headers:bearerHeaders(token),signal})
}

export interface CutoutStatus { available:boolean; engine:string; stillImages:boolean; video:boolean }
export type CutoutSummary = Omit<ImageEnhancementSummary,'factor'>
export function getCutoutStatus(token:string,signal?:AbortSignal):Promise<CutoutStatus>{
 return requestJson('/api/cutout/status',{headers:bearerHeaders(token),signal})
}
export function startCutout(token:string,sourceId:string,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/cutout/jobs',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({sourceId}),signal})
}
export async function getCutoutImage(token:string,id:string,signal?:AbortSignal):Promise<Blob>{
 const response=await safeFetch(`/api/cutout/jobs/${encodeURIComponent(id)}/image`,{headers:bearerHeaders(token),signal})
 await requireOk(response,'Не удалось загрузить изображение без фона')
 if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG')
 const blob=await response.blob()
 if(blob.size<8||blob.size>256*1024*1024)throw new Error('Недопустимый размер изображения')
 return blob
}
export function saveCutoutImage(token:string,id:string,projectId:string,signal?:AbortSignal):Promise<CutoutAcceptedMedia>{
 return requestJson(`/api/cutout/jobs/${encodeURIComponent(id)}/asset`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({projectId}),signal})
}
export function listCutouts(token:string,sourceId?:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:CutoutSummary[];nextCursor:string|null}>{
 const query=new URLSearchParams();if(sourceId)query.set('sourceId',sourceId);if(cursor)query.set('cursor',cursor)
 return requestJson(`/api/cutout/jobs?${query}`,{headers:bearerHeaders(token),signal})
}

export function listCutoutModels(token:string,signal?:AbortSignal):Promise<CutoutModelLibrary>{
 return requestJson('/api/cutout/models',{headers:bearerHeaders(token),signal})
}
export function selectCutoutModel(token:string,modelId:string,signal?:AbortSignal):Promise<{selectedId:string}>{
 return requestJson('/api/cutout/models/select',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({modelId}),signal})
}
export function removeCutoutModel(token:string,modelId:string,signal?:AbortSignal):Promise<{removed:boolean;fileReleased:boolean}>{
 return requestJson('/api/cutout/models/remove',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({modelId}),signal})
}
export function startCutoutModelDownload(token:string,catalogId:string,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/cutout/models/downloads',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({catalogId}),signal})
}
export function listCutoutModelDownloads(token:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:SpeechModelDownloadSummary[];nextCursor:string|null}>{
 return requestJson('/api/cutout/models/downloads'+(cursor?'?cursor='+encodeURIComponent(cursor):''),{headers:bearerHeaders(token),signal})
}

export type CutoutModel=EnhancementModel
export type CutoutModelLibrary=EnhancementModelLibrary
export function listSamModels(token:string,signal?:AbortSignal):Promise<SamModelLibrary>{
 return requestJson('/api/sam/models',{headers:bearerHeaders(token),signal})
}
export function selectSamModel(token:string,modelId:string,signal?:AbortSignal):Promise<{selectedId:string}>{
 return requestJson('/api/sam/models/select',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({modelId}),signal})
}
export function removeSamModel(token:string,modelId:string,signal?:AbortSignal):Promise<{removed:boolean;fileReleased:boolean}>{
 return requestJson('/api/sam/models/remove',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({modelId}),signal})
}
export function startSamModelDownload(token:string,catalogId:string,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/sam/models/downloads',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({catalogId}),signal})
}
export function listSamModelDownloads(token:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:SpeechModelDownloadSummary[];nextCursor:string|null}>{
 return requestJson('/api/sam/models/downloads'+(cursor?'?cursor='+encodeURIComponent(cursor):''),{headers:bearerHeaders(token),signal})
}

export type SamModel=EnhancementModel
export type SamModelLibrary=EnhancementModelLibrary

export type {MaskStroke,MaskCorrection} from './composition/types'
import type {MaskCorrection} from './composition/types'
export interface CutoutAcceptedMedia extends MediaEntry {cutout?:{snapshot:import('./composition/types').CutoutAttachment;source:MediaEntry;mask:MediaEntry;regions?:MediaEntry[]}}
export async function getCutoutOriginal(token:string,id:string,signal?:AbortSignal):Promise<Blob>{
 const response=await safeFetch(`/api/cutout/jobs/${encodeURIComponent(id)}/original`,{headers:bearerHeaders(token),signal});await requireOk(response,'Не удалось открыть исходник маски');if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG');const blob=await response.blob();if(blob.size<8||blob.size>16*1024*1024)throw new Error('Недопустимый размер превью');return blob
}
export async function getCutoutCorrection(token:string,id:string,signal?:AbortSignal):Promise<{baseJobId:string;correction:MaskCorrection}|null>{
 const response=await safeFetch(`/api/cutout/jobs/${encodeURIComponent(id)}/correction`,{headers:bearerHeaders(token),signal});if(response.status===404)return null;await requireOk(response,'Не удалось прочитать правки маски');return response.json()
}
export function startCutoutCorrection(token:string,id:string,correction:MaskCorrection,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson(`/api/cutout/jobs/${encodeURIComponent(id)}/corrections`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify(correction),signal})
}

/** Correct a saved project mask independently of inference job history. */
export function startProjectCutoutCorrection(token:string,projectId:string,clipId:string,correction:MaskCorrection,compositionId?:string,signal?:AbortSignal,expectedAttachment?:import('./composition/types').CutoutAttachment):Promise<{jobId:string}>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/cutout/corrections`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({clipId,correction,...(compositionId===undefined?{}:{compositionId}),...(expectedAttachment?{expectedAttachment}:{})}),signal})
}

export interface ProjectCutoutContext {projectId:string;clipId:string;compositionId?:string;snapshot:import('./composition/types').CutoutAttachment}
function projectCutoutUrl(context:ProjectCutoutContext){return `/api/composition-projects/${encodeURIComponent(context.projectId)}/cutout/${encodeURIComponent(context.clipId)}`}
function projectCutoutQuery(context:ProjectCutoutContext){return context.compositionId===undefined?'':`?compositionId=${encodeURIComponent(context.compositionId)}`}
export function getProjectCutoutSettings(token:string,context:ProjectCutoutContext,signal?:AbortSignal):Promise<{snapshot:import('./composition/types').CutoutAttachment;width:number;height:number}>{
 return requestJson(projectCutoutUrl(context)+projectCutoutQuery(context),{headers:bearerHeaders(token),signal})
}
export async function getProjectCutoutOriginal(token:string,context:ProjectCutoutContext,signal?:AbortSignal):Promise<Blob>{
 const response=await safeFetch(projectCutoutUrl(context)+'/original'+projectCutoutQuery(context),{headers:bearerHeaders(token),signal});await requireOk(response,'Не удалось открыть исходник маски проекта');if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG');const blob=await response.blob();if(blob.size<8||blob.size>16*1024*1024)throw new Error('Недопустимый размер превью');return blob
}

export function getSamStatus(token:string,signal?:AbortSignal):Promise<CutoutStatus>{
 return requestJson('/api/sam/status',{headers:bearerHeaders(token),signal})
}
export function startSamRegion(token:string,sourceId:string,points:readonly [number,number][],signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/sam/jobs',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({sourceId,points}),signal})
}
export function listSamRegions(token:string,sourceId?:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:CutoutSummary[];nextCursor:string|null}>{
 const query=new URLSearchParams();if(sourceId)query.set('sourceId',sourceId);if(cursor)query.set('cursor',cursor)
 return requestJson('/api/sam/jobs?'+query,{headers:bearerHeaders(token),signal})
}
export function getSamPoints(token:string,id:string,signal?:AbortSignal):Promise<{sourceId:string;points:[number,number][]}>{
 return requestJson(`/api/sam/jobs/${encodeURIComponent(id)}/points`,{headers:bearerHeaders(token),signal})
}
/** The seed is a decoder ordinal in the original source, independent of clip retiming. */
export function startSamVideoTracking(token:string,sourceId:string,points:readonly [number,number][],seedFrame:number,signal?:AbortSignal):Promise<{jobId:string}>{
 if(!Number.isInteger(seedFrame)||seedFrame<0||seedFrame>=360000)throw new Error('Недопустимый номер исходного кадра')
 return requestJson('/api/sam/jobs',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({sourceId,points,seedFrame}),signal})
}
export function listSamVideoTracking(token:string,sourceId?:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:CutoutSummary[];nextCursor:string|null}>{
 const query=new URLSearchParams({video:'true'});if(sourceId)query.set('sourceId',sourceId);if(cursor)query.set('cursor',cursor)
 return requestJson('/api/sam/jobs?'+query,{headers:bearerHeaders(token),signal})
}
export interface SavedSamVideoRegion {source:MediaEntry;mask:MediaEntry;snapshot:import('./composition/types').SamVideoRegionAttachment}
export function saveSamVideoRegion(token:string,id:string,projectId:string,signal?:AbortSignal):Promise<SavedSamVideoRegion>{
 return requestJson(`/api/sam/jobs/${encodeURIComponent(id)}/video-asset`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({projectId}),signal})
}
export function getSamVideoPoints(token:string,id:string,signal?:AbortSignal):Promise<{sourceId:string;points:[number,number][];seedFrame:number}>{
 return requestJson(`/api/sam/jobs/${encodeURIComponent(id)}/points`,{headers:bearerHeaders(token),signal})
}

export async function getSamMask(token:string,id:string,signal?:AbortSignal):Promise<Blob>{
 const response=await safeFetch(`/api/sam/jobs/${encodeURIComponent(id)}/mask`,{headers:bearerHeaders(token),signal});await requireOk(response,'Не удалось открыть выделенную область');if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG');const blob=await response.blob();if(blob.size<8||blob.size>1024*1024)throw new Error('Недопустимый размер маски');return blob
}

export interface SavedSamRegion extends MediaEntry {region:import('./composition/types').SamRegionAttachment;source:MediaEntry}
export function saveSamRegion(token:string,id:string,projectId:string,signal?:AbortSignal):Promise<SavedSamRegion>{
 return requestJson(`/api/sam/jobs/${encodeURIComponent(id)}/asset`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({projectId}),signal})
}

export function getSelfieStatus(token:string,signal?:AbortSignal):Promise<CutoutStatus>{
 return requestJson('/api/selfie/status',{headers:bearerHeaders(token),signal})
}
export function startSelfie(token:string,sourceId:string,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/selfie/jobs',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({sourceId}),signal})
}
export async function getSelfieImage(token:string,id:string,signal?:AbortSignal):Promise<Blob>{
 const response=await safeFetch(`/api/selfie/jobs/${encodeURIComponent(id)}/image`,{headers:bearerHeaders(token),signal})
 await requireOk(response,'Не удалось загрузить изображение без фона')
 if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG')
 const blob=await response.blob()
 if(blob.size<8||blob.size>256*1024*1024)throw new Error('Недопустимый размер изображения')
 return blob
}
export function listSelfies(token:string,sourceId?:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:CutoutSummary[];nextCursor:string|null}>{
 const query=new URLSearchParams();if(sourceId)query.set('sourceId',sourceId);if(cursor)query.set('cursor',cursor)
 return requestJson(`/api/selfie/jobs?${query}`,{headers:bearerHeaders(token),signal})
}
export async function getSelfieOriginal(token:string,id:string,signal?:AbortSignal):Promise<Blob>{
 const response=await safeFetch(`/api/selfie/jobs/${encodeURIComponent(id)}/original`,{headers:bearerHeaders(token),signal});await requireOk(response,'Не удалось открыть исходник маски');if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG');const blob=await response.blob();if(blob.size<8||blob.size>16*1024*1024)throw new Error('Недопустимый размер превью');return blob
}
export function startSelfieCorrection(token:string,id:string,correction:MaskCorrection,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson(`/api/selfie/jobs/${encodeURIComponent(id)}/corrections`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify(correction),signal})
}
export function saveSelfieImage(token:string,id:string,projectId:string,signal?:AbortSignal):Promise<CutoutAcceptedMedia>{
 return requestJson(`/api/selfie/jobs/${encodeURIComponent(id)}/asset`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({projectId}),signal})
}

export interface PersonVideoStatus {available:boolean;correctionAvailable?:boolean;model:string;video:boolean;alpha:boolean;timing:string;color:string;maxComponentBits:number}
export function getPersonVideoStatus(token:string,signal?:AbortSignal):Promise<PersonVideoStatus>{
 return requestJson('/api/person/status',{headers:bearerHeaders(token),signal})
}
export function startPersonVideo(token:string,sourceId:string,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson('/api/person/video-jobs',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({sourceId}),signal})
}
export interface SavedPersonVideo extends MediaEntry {videoCutout?:{snapshot:import('./composition/types').VideoMaskAttachment;source:MediaEntry;mask:MediaEntry}}
export function savePersonVideo(token:string,id:string,projectId:string,signal?:AbortSignal):Promise<SavedPersonVideo>{
 return requestJson('/api/person/video-jobs/'+encodeURIComponent(id)+'/asset',{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({projectId}),signal})
}
export function listPersonVideos(token:string,sourceId?:string,cursor?:string,signal?:AbortSignal):Promise<{jobs:CutoutSummary[];nextCursor:string|null}>{
 const query=new URLSearchParams();if(sourceId)query.set('sourceId',sourceId);if(cursor)query.set('cursor',cursor)
 return requestJson('/api/person/video-jobs?'+query,{headers:bearerHeaders(token),signal})
}
export type {VideoFrameCorrection} from './composition/types'
import type {VideoFrameCorrection} from './composition/types'
export interface VideoCorrectionRecipe {baseJobId:string;maskHash:string;corrections:VideoFrameCorrection[];projectOrigin?:{projectId:string;clipId:string;compositionId?:string|null;snapshot:import('./composition/types').VideoMaskAttachment;savedSnapshot?:import('./composition/types').VideoMaskAttachment}}
export function getVideoCorrectionRecipe(token:string,id:string,signal?:AbortSignal):Promise<VideoCorrectionRecipe>{
 return requestJson(`/api/person/video-jobs/${encodeURIComponent(id)}/corrections`,{headers:bearerHeaders(token),signal})
}
export function startVideoCorrection(token:string,id:string,corrections:VideoFrameCorrection[],signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson(`/api/person/video-jobs/${encodeURIComponent(id)}/corrections`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({corrections}),signal})
}
export async function getVideoMaskFrame(token:string,id:string,frame:number,signal?:AbortSignal):Promise<Blob>{
 if(!Number.isSafeInteger(frame)||frame<0||frame>=360000)throw new Error('Неверный кадр маски')
 const response=await safeFetch(`/api/person/video-jobs/${encodeURIComponent(id)}/masks/${frame}`,{headers:bearerHeaders(token),signal});await requireOk(response,'Не удалось открыть маску кадра');if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG');const blob=await response.blob();if(blob.size<8||blob.size>1024*1024)throw new Error('Недопустимый размер маски');return blob
}

export async function getVideoRegionFrame(token:string,id:string,regionId:string,frame:number,signal?:AbortSignal):Promise<{png:Blob;maskHash:string}>{
 if(!Number.isSafeInteger(frame)||frame<0||frame>=360000)throw new Error('Неверный кадр области')
 const response=await safeFetch(`/api/person/video-jobs/${encodeURIComponent(id)}/regions/${encodeURIComponent(regionId)}/${frame}`,{headers:bearerHeaders(token),signal});await requireOk(response,'Не удалось открыть область кадра')
 if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG области')
 const maskHash=response.headers.get('x-mask-sha256');if(!maskHash||!/^[a-f0-9]{64}$/.test(maskHash)||response.headers.get('x-source-frame')!==String(frame))throw new Error('Сервер вернул область другого кадра')
 const png=await response.blob();if(png.size<8||png.size>1024*1024)throw new Error('Недопустимый размер области');return {png,maskHash}
}

/** Fork a project-origin correction with a completed SAM tracking result, without saving its document. */
export function continueVideoWithRegion(token:string,id:string,samJobId:string,corrections:VideoFrameCorrection[],signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson(`/api/person/video-jobs/${encodeURIComponent(id)}/regions`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({samJobId,corrections}),signal})
}

export interface ProjectVideoMaskContext {clipId:string;compositionId?:string;snapshot:import('./composition/types').VideoMaskAttachment}
export function startProjectVideoCorrection(token:string,projectId:string,context:ProjectVideoMaskContext,signal?:AbortSignal):Promise<{jobId:string}>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/video-cutout/corrections`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({clipId:context.clipId,...(context.compositionId===undefined?{}:{compositionId:context.compositionId}),expectedAttachment:context.snapshot,corrections:context.snapshot.corrections}),signal})
}

export async function getVideoSourceFrame(token:string,id:string,frame:number,signal?:AbortSignal):Promise<Blob>{
 if(!Number.isSafeInteger(frame)||frame<0||frame>=360000)throw new Error('Неверный исходный кадр')
 const response=await safeFetch(`/api/person/video-jobs/${encodeURIComponent(id)}/source/${frame}`,{headers:bearerHeaders(token),signal});await requireOk(response,'Не удалось открыть исходный кадр');if(!response.headers.get('content-type')?.startsWith('image/png'))throw new Error('Сервер не вернул PNG');const blob=await response.blob();if(blob.size<8||blob.size>272*1024*1024)throw new Error('Недопустимый размер исходного кадра');return blob
}

/** Exact 48 kHz stereo sample coordinates; PCM is explicitly little endian. */
export async function renderCompositionAudioPreview(
  project: NativeNestedProject,
  startSample: number,
  frames: number,
  token?: string,
  signal?: AbortSignal,
  compositionId?: string,
): Promise<Float32Array> {
  if (!Number.isSafeInteger(startSample) || startSample < 0 || !Number.isInteger(frames) || frames < 1 || frames > 96000) {
    throw new RangeError('Некорректный диапазон аудиопредпросмотра')
  }
  let response: Response
  try {
    response = await fetch('/api/compositions/audio-preview', {
      method: 'POST',
      headers: { ...(token ? bearerHeaders(token) : {}), 'Content-Type': 'application/json' },
      body: JSON.stringify({ project, startSample, frames, compositionId }), signal,
    })
  } catch (error) {
    if (signal?.aborted) throw error
    throw new BackendUnavailableError()
  }
  await requireOk(response, 'Не удалось подготовить звук композиции')
  if (response.headers.get('content-type')?.split(';')[0] !== 'application/octet-stream' ||
      response.headers.get('x-audio-rate') !== '48000' || response.headers.get('x-audio-channels') !== '2' ||
      response.headers.get('x-audio-frames') !== String(frames) ||
      response.headers.get('x-audio-start-sample') !== String(startSample)) {
    throw new ApiError('Некорректный формат аудиопредпросмотра', response.status)
  }
  const buffer = await response.arrayBuffer()
  if (buffer.byteLength !== frames * 8) throw new ApiError('Неполный PCM-блок', response.status)
  const view = new DataView(buffer)
  const samples = new Float32Array(frames * 2)
  for (let i = 0; i < samples.length; i++) {
    const value = view.getFloat32(i * 4, true)
    if (!Number.isFinite(value)) throw new ApiError('Некорректный PCM-сэмпл', response.status)
    samples[i] = value
  }
  return samples
}

/** Original Concat links; native conversion preserves bypass and keyed values. */
export interface ConcatEffectLink {
  readonly id: string
  readonly params?: Readonly<Record<string,number>>
  readonly enabled?: boolean
  readonly keys?: Readonly<Record<string,readonly {readonly at:number;readonly value:number;readonly ease?:string|readonly [number,number,number,number]}[]>>
}
export interface ConcatNativeTransition {
  readonly kind: import('./composition/types').CompositionTransitionKind
  readonly kernel?: import('./composition/transitionCatalog').ProceduralTransitionSpec
}
export interface ConcatEffectUpgradeResult {
  readonly schemaVersion: 1
  readonly nativeEffects?: readonly (WireVideoEffect|null)[]
  readonly nativeTransitions?: readonly (ConcatNativeTransition|null)[]
  readonly links: readonly {readonly upgraded:boolean;readonly link:ConcatEffectLink}[]
}
/** Converts retired links. Applying a whole project or its animation is separate. */
export function upgradeConcatEffectLinks(links:readonly ConcatEffectLink[],signal?:AbortSignal,includeNativeEffects=false,includeNativeTransitions=false):Promise<ConcatEffectUpgradeResult> {
  return requestJson('/api/compositions/concat-effects/upgrade',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({schemaVersion:1,links,includeNativeEffects,includeNativeTransitions}),signal})
}

/** Convert a Concat v1 document using explicitly registered/relinked media. No source path is opened. */
export interface ConcatProjectImportReport {
  readonly schemaVersion:1
  readonly status:'converted'|'needs_attention'
  readonly project:NativeNestedProject|null
  readonly diagnostics:readonly string[]
  readonly sourceDocument:unknown
  readonly timelineIds:Readonly<Record<string,string>>
}
export interface RegisteredConcatSlotFill { readonly document:unknown; readonly source:import('./composition/types').CompositionSource }
/** Fill from an ACL-checked registered source and fresh server probe. */
export async function fillConcatMediaSlot(document:unknown,mediaId:string,entry:{label:string;source:import('./composition/types').CompositionSource},signal?:AbortSignal,token?:string|null):Promise<RegisteredConcatSlotFill>{
 const reply=await requestJson<{schemaVersion:number;document:unknown;applied:boolean;source:import('./composition/types').CompositionSource}>('/api/compositions/concat-project/fill-registered-slot',{method:'POST',headers:{'Content-Type':'application/json',...(token?{Authorization:`Bearer ${token}`}:{})},body:JSON.stringify({document,mediaId,sourceId:entry.source.id}),signal})
 if(signal?.aborted)throw new DOMException('Aborted','AbortError')
 if(reply.schemaVersion!==1||reply.applied!==true||!reply.document||typeof reply.document!=='object'||reply.source?.id!==entry.source.id||!['video','audio','image'].includes(reply.source.kind)||!Number.isSafeInteger(reply.source.durationTicks))throw new Error('Некорректный ответ заполнения слота Concat')
 return {document:reply.document,source:reply.source}
}

/** Saved server-owned cutout, addressed independently from its destination. */
export interface ConcatSavedStillReference {
 readonly timelineId:string;readonly clipId:string;readonly projectId:string;readonly compositionId?:string|null;readonly sourceClipId:string;readonly expectedRevision:number
}
export type ConcatSavedVideoReference=ConcatSavedStillReference
/** Reverify saved private bundles; client mask metadata/paths are never authority. */
export function importConcatProjectWithSavedCutouts(document:unknown,mediaBindings:Readonly<Record<string,import('./composition/types').CompositionSource>>,preparedStills:readonly ConcatSavedStillReference[],preparedVideos:readonly ConcatSavedVideoReference[],signal?:AbortSignal,token?:string|null,fontBindings:Readonly<Record<string,ConcatFontBinding>>={},preserveMediaIds=false):Promise<ConcatProjectImportReport>{
 return requestJson('/api/compositions/concat-project/import-ready',{method:'POST',headers:{'Content-Type':'application/json',...(token?{Authorization:`Bearer ${token}`}:{})},body:JSON.stringify({request:{schemaVersion:1,document,mediaBindings,preserveMediaIds,...(Object.keys(fontBindings).length?{fontBindings}:{})},preparedStills,...(preparedVideos.length?{preparedVideos}:{})}),signal})
}

export function importConcatProjectWithSavedStills(document:unknown,mediaBindings:Readonly<Record<string,import('./composition/types').CompositionSource>>,preparedStills:readonly ConcatSavedStillReference[],signal?:AbortSignal,token?:string|null,fontBindings:Readonly<Record<string,ConcatFontBinding>>={},preserveMediaIds=false):Promise<ConcatProjectImportReport>{
 return importConcatProjectWithSavedCutouts(document,mediaBindings,preparedStills,[],signal,token,fontBindings,preserveMediaIds)
}

export type ConcatFontBinding={readonly data:string}|{readonly fallback:true}
export function importConcatProject(document:unknown,mediaBindings:Readonly<Record<string,import('./composition/types').CompositionSource>>,signal?:AbortSignal,token?:string|null,fontBindings:Readonly<Record<string,ConcatFontBinding>>={},preserveMediaIds=false):Promise<ConcatProjectImportReport>{
 return requestJson('/api/compositions/concat-project/import',{method:'POST',headers:{'Content-Type':'application/json',...(token?{Authorization:`Bearer ${token}`}:{})},body:JSON.stringify({schemaVersion:1,document,mediaBindings,preserveMediaIds,...(Object.keys(fontBindings).length?{fontBindings}:{})}),signal})
}


export interface SourceAudioStream {
 index:number;codec:string|null;channels:number|null;sampleRate:number|null;channelLayout:string|null;language:string|null;title:string|null;default:boolean
}
export async function getSourceAudioStreams(sourceId:string,token?:string,signal?:AbortSignal):Promise<{sourceId:string;streams:SourceAudioStream[]}>{
 return requestJson(`/api/library/${encodeURIComponent(sourceId)}/audio-streams`,{headers:token?{Authorization:`Bearer ${token}`}:{},signal})
}

export type ConcatCutoutSubject='auto'|'person'|'object'
export function analyseNestedCutout(projectId:string,compositionId:string,clipId:string,expectedRevision:number,subject:ConcatCutoutSubject,feather:number,token:string,signal?:AbortSignal,correction?:MaskCorrection):Promise<NestedCompositionProjectDto>{
 const cutout={mode:correction?.custom?'custom':'auto',subject,feather,...(correction?{strokes:correction.strokes.map(({tool,size,points,at})=>({tool,size,points,at}))}:{})}
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/concat-cutout`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({compositionId,clipId,expectedRevision,cutout}),signal})
}

export interface ConcatVideoAnalysis {subject?:ConcatCutoutSubject;modelHash?:string|null;sourceId:string;analysedRuns:number;analysedFrames:number;emptyRuns:number;cacheReused:boolean}
export async function analyseNestedVideoCutout(projectId:string,compositionId:string,clipId:string,expectedRevision:number,token:string,signal?:AbortSignal,subject:ConcatCutoutSubject='person'):Promise<ConcatVideoAnalysis>{
 if(signal?.aborted)throw new DOMException('Video analysis cancelled','AbortError')
 const operationId=crypto.randomUUID(),url=`/api/composition-projects/${encodeURIComponent(projectId)}/concat-video-cutout`
 const cancel=()=>{void safeFetch(`${url}/cancel`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({operationId}),keepalive:true,signal:AbortSignal.timeout(10000)}).catch(()=>{})}
 signal?.addEventListener('abort',cancel,{once:true})
 try{return await requestJson(`${url}/analyse`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({compositionId,clipId,expectedRevision,subject,operationId}),signal})}
 finally{signal?.removeEventListener('abort',cancel)}
}
export async function previewNestedVideoCutout(projectId:string,compositionId:string,clipId:string,expectedRevision:number,timeUs:number,token:string,signal?:AbortSignal,feather?:number,correction?:MaskCorrection,sourceSpace?:boolean,sourceOriginal?:boolean,regionTracks?:readonly import('./composition/types').ConcatVideoRegionTrack[]):Promise<NativeNestedPreviewFrame>{
 const response=await fetch(`/api/composition-projects/${encodeURIComponent(projectId)}/concat-video-cutout/preview`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({compositionId,clipId,expectedRevision,timeUs,feather,correction,sourceSpace,sourceOriginal,regionTracks}),signal})
 await requireOk(response,'Не удалось показать удаление фона видео')
 const width=Number(response.headers.get('x-frame-width')),height=Number(response.headers.get('x-frame-height'))
 if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||width>3840||height>2160)throw new ApiError('Некорректный размер видеокадра',response.status)
 const bytes=await response.arrayBuffer()
 if(bytes.byteLength!==width*height*4)throw new ApiError('Неполный RGBA-видеокадр',response.status)
 return {width,height,rgba:new Uint8ClampedArray(bytes)}
}

export function setNestedVideoCutoutBinding(projectId:string,compositionId:string,clipId:string,expectedRevision:number,enabled:boolean,token:string,signal?:AbortSignal,subject?:ConcatCutoutSubject,expectedModelHash?:string,feather?:number,correction?:MaskCorrection,regionTracks?:readonly import('./composition/types').ConcatVideoRegionTrack[]):Promise<NestedCompositionProjectDto>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/concat-video-cutout/binding`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({compositionId,clipId,expectedRevision,enabled,subject,expectedModelHash,feather,correction,regionTracks}),signal})
}

export interface NestedVideoSmartRegionJob {frameTimesUs?:readonly number[];jobId:string;sourceId:string;sourceUs:number;seedFrame:number;frames:number;frameRate:{numerator:number;denominator:number};expectedRevision:number}
export function startNestedVideoSmartRegion(projectId:string,compositionId:string,clipId:string,expectedRevision:number,timeUs:number,points:readonly [number,number][],token:string,signal?:AbortSignal,operationId?:string):Promise<NestedVideoSmartRegionJob>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/concat-video-cutout/smart-region`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({compositionId,clipId,expectedRevision,timeUs,points,operationId}),signal})
}

export function attachNestedVideoSmartRegion(projectId:string,compositionId:string,clipId:string,expectedRevision:number,timeUs:number,jobId:string,tool:'smartBrush'|'smartEraser',size:number,token:string,signal?:AbortSignal):Promise<NestedCompositionProjectDto>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/concat-video-cutout/smart-region/attach`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify({compositionId,clipId,expectedRevision,timeUs,jobId,tool,size}),signal})
}

/** Observe owned durable SAM work; stopping observation leaves the job intact. */
export async function observeNestedVideoSmartRegion(token:string,jobId:string,signal?:AbortSignal,onJob?:(job:Job)=>void):Promise<Job>{
 const deadline=Date.now()+50*60*1000
 while(true){
  signal?.throwIfAborted()
  const job=await requestJson<Job>(`/api/jobs/${encodeURIComponent(jobId)}`,{headers:bearerHeaders(token),signal})
  signal?.throwIfAborted()
  if(job.id!==jobId||!['pending','running','done','error','cancelled','interrupted'].includes(job.status))throw new Error('Сервер вернул другую задачу SAM')
  onJob?.(job)
  if(job.status==='done')return job
  if(job.status==='error'||job.status==='interrupted'||job.status==='cancelled')throw new Error(job.error||'Обработка области SAM остановлена')
  if(Date.now()>=deadline)throw new Error('Превышено время ожидания области SAM')
  await new Promise<void>((resolve,reject)=>{
   const abort=()=>{clearTimeout(timer);signal?.removeEventListener('abort',abort);reject(signal?.reason??new DOMException('Aborted','AbortError'))}
   const timer=setTimeout(()=>{signal?.removeEventListener('abort',abort);resolve()},500)
   signal?.addEventListener('abort',abort,{once:true});if(signal?.aborted)abort()
  })
 }
}
export function cancelNestedVideoSmartRegion(token:string,jobId:string):Promise<unknown>{
 return requestJson(`/api/jobs/${encodeURIComponent(jobId)}/cancel`,{method:'POST',headers:bearerHeaders(token),signal:AbortSignal.timeout(10000)})
}

/** A durable server receipt can exist before the SAM job is enqueued. */
export interface NestedVideoSmartOperation {operationId:string;jobId:string;started:NestedVideoSmartRegionJob|null;cancelled:boolean;enqueued:boolean}
export function getNestedVideoSmartOperation(projectId:string,operationId:string,token:string,signal?:AbortSignal):Promise<NestedVideoSmartOperation>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/concat-video-cutout/smart-region/operations/${encodeURIComponent(operationId)}`,{headers:bearerHeaders(token),signal})
}
export function cancelNestedVideoSmartOperation(projectId:string,operationId:string,token:string):Promise<unknown>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/concat-video-cutout/smart-region/operations/${encodeURIComponent(operationId)}/cancel`,{method:'POST',headers:bearerHeaders(token),signal:AbortSignal.timeout(10000)})
}

export function exportConcatProject(project:NativeNestedProject,name:string,signal?:AbortSignal,mediaPaths:Record<string,string>={}):Promise<{schemaVersion:1;document:unknown}>{
 return requestJson('/api/compositions/concat-project/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({project,name,mediaPaths}),signal})
}

export interface Insta360SourceMetadata {readonly version:number;readonly cameraModel:string|null;readonly firmware:string|null;readonly frameRate:number|null;readonly dimension:readonly [number,number]|null;readonly resolution:readonly [number,number]|null;readonly rollingShutterTime?:number|null;readonly windowCropInfo?:{readonly sourceSize:readonly [number,number];readonly destinationSize:readonly [number,number];readonly offset:readonly [number,number]}|null;readonly calibrations:readonly {fieldNumber:number;value:string}[];readonly blocks:readonly {kind:number;format:number;length:number;offset:number}[]}
export function inspectInsta360Source(id:string,token:string,signal?:AbortSignal):Promise<Insta360SourceMetadata|null>{return requestJson(`/api/library/${encodeURIComponent(id)}/insta360`,{headers:{Authorization:`Bearer ${token}`},signal})}

export type FilmcraftScalarValue = { Float: number } | { Choice: number } | { Bool: boolean }
export interface FilmcraftPathPoint { x: number; y: number; [extension: string]: unknown }
export interface FilmcraftPathVertex { p: FilmcraftPathPoint; t_in?: FilmcraftPathPoint; t_out?: FilmcraftPathPoint; [extension: string]: unknown }
export interface FilmcraftPath { vertices: FilmcraftPathVertex[]; closed?: boolean; [extension: string]: unknown }
export type FilmcraftParameterValue = FilmcraftScalarValue | { Vec2: { x: number | null; y: number | null } } | { Color: [number, number, number, number] } | { Text: string } | { Curve: [number, number][] } | { Path: FilmcraftPath }
export type FilmcraftParameterCommand =
  | { kind: 'setBase' | 'atPlayhead'; value: FilmcraftParameterValue }
  | { kind: 'upsert'; time: string; value: FilmcraftParameterValue }
  | { kind: 'delete'; time: string }
  | { kind: 'move'; time: string; destination: string }
  | { kind: 'interpolation'; time: string; interpolation: string }
  | { kind: 'influence'; time: string; incoming: number; outgoing: number }
export interface FilmcraftParameterInfo {
  name: string; value: FilmcraftParameterValue; sampled: number | null; sampledComponents?: number[] | null; sampledValue?: FilmcraftParameterValue | null
  keys: Array<{ time: string; value: FilmcraftParameterValue; interp: string; in_influence?: number; out_influence?: number }>
}
export interface FilmcraftParameterReport {
  locked: boolean; timelineTicks: string; parameterTicks: string; frameTicks: string; generatorTicks: string
  effects: Array<{ index: number; kind: string; enabled: boolean; maskable?: boolean; parameters: FilmcraftParameterInfo[]; masks?: FilmcraftMaskInfo[] }>
}
type FilmcraftParameterClock = { timeTicks: string; timeFrames?: never } | { timeFrames: string; timeTicks?: never }
export function getFilmcraftParameters(input: { document: string; sequenceId: string; clipId: string } & FilmcraftParameterClock, signal?: AbortSignal): Promise<FilmcraftParameterReport> {
  return requestJson('/api/compositions/filmcraft/parameters', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}
export function editFilmcraftParameter(input: { document: string; address: { sequenceId: string; clipId: string; effectIndex: number; effectKind: string; parameter: string; maskIndex?: number }; command: FilmcraftParameterCommand } & FilmcraftParameterClock, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  return requestJson('/api/compositions/filmcraft/edit-parameter', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export type FilmcraftMaskMode = 'None' | 'Add' | 'Subtract' | 'Intersect' | 'Lighten' | 'Darken' | 'Difference'
export type FilmcraftMaskTrackMethod = 'Position' | 'PositionRotation' | 'PositionScaleRotation'
export interface FilmcraftMaskInfo { index: number; name: string | null; mode?: FilmcraftMaskMode; inverted?: boolean; trackMethod?: FilmcraftMaskTrackMethod; parameters: FilmcraftParameterInfo[] }
export type FilmcraftMaskShape = { kind: 'rectangle' | 'ellipse'; x0: number; y0: number; x1: number; y1: number } | { kind: 'freeDraw' } | { kind: 'path'; path: FilmcraftPath }
export type FilmcraftMaskCommand =
  | { kind: 'create'; name: string; shape: FilmcraftMaskShape }
  | { kind: 'delete'; maskIndex: number }
  | { kind: 'move'; maskIndex: number; destination: number }
  | { kind: 'settings'; maskIndex: number; name?: string; mode?: FilmcraftMaskMode; inverted?: boolean; trackMethod?: FilmcraftMaskTrackMethod }
export function editFilmcraftMask(input: { document: string; address: { sequenceId: string; clipId: string; effectIndex: number; effectKind: string }; command: FilmcraftMaskCommand }, signal?: AbortSignal): Promise<FilmcraftImportResult> {
  return requestJson('/api/compositions/filmcraft/edit-mask', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export interface FilmcraftMaskTrackingResult extends FilmcraftImportResult { tracking: { status: 'tracked' | 'lost' | 'boundary'; sourceTicks: string; more: boolean; inliers: number } }
export function trackFilmcraftMask(input: { document: string; address: { sequenceId: string; clipId: string; effectIndex: number; effectKind: string; maskIndex: number; parameter: 'path' }; sources: Record<string, string>; direction: 'forward' | 'backward'; cursorTicks?: string } & FilmcraftParameterClock, token?: string | null, signal?: AbortSignal): Promise<FilmcraftMaskTrackingResult> {
  return requestJson('/api/compositions/filmcraft/track-mask', { method: 'POST', headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) }, body: JSON.stringify(input), signal })
}

export type FilmcraftMaskMonitorPart = 'vertex' | 'in' | 'out'
export type FilmcraftMaskMonitorOperation =
  | { kind: 'read' }
  | { kind: 'bodyClick'; point: [number, number]; pixelScale: [number, number] }
  | { kind: 'move'; index: number; part: FilmcraftMaskMonitorPart; point: [number, number]; breakHandles?: boolean }
  | { kind: 'translate'; delta: [number, number]; origin?: [number, number] }
  | { kind: 'falloff'; parameter: 'feather' | 'expansion'; delta: [number, number] }
  | { kind: 'append'; point: [number, number] }
  | { kind: 'penDraft'; points: Array<{ point: [number, number]; handle?: [number, number] }>; closed: boolean }
  | { kind: 'insertNear'; point: [number, number]; pixelScale: [number, number] }
  | { kind: 'delete' | 'smooth'; index: number }
  | { kind: 'close'; closed: boolean }
  | { kind: 'box'; shape: 'rectangle' | 'ellipse'; from: [number, number]; to: [number, number] }
export interface FilmcraftMaskFalloffHandle { parameter: 'feather' | 'expansion'; point: [number, number]; direction: [number, number] }
export interface FilmcraftMaskOverlay { deselectMask?: boolean; falloff?: { feather: number; expansion: number; expansionContour: Array<[number, number]>; featherContour: Array<[number, number]>; handles: FilmcraftMaskFalloffHandle[] }; createdMaskIndex?: number; selectedVertex?: number; canvas: [number, number]; matrix: [number, number, number, number, number, number]; sampleTicks: string; locked: boolean; visible: boolean; editable: boolean; closed: boolean; svgPath: string; vertices: Array<{ index: number; point: [number, number]; incoming: [number, number]; outgoing: [number, number] }> }
export interface FilmcraftMaskMonitorResult { overlay: FilmcraftMaskOverlay; project: FilmcraftImportResult | null }
export function editFilmcraftMaskMonitor(input: { document: string; createMask?: boolean; address: { sequenceId: string; clipId: string; effectIndex: number; effectKind: string; maskIndex?: number; parameter: 'path' }; operation: FilmcraftMaskMonitorOperation } & FilmcraftParameterClock, signal?: AbortSignal): Promise<FilmcraftMaskMonitorResult> {
  return requestJson('/api/compositions/filmcraft/mask-monitor', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal })
}

export interface Insta360GyroRequest {
 readonly startSeconds:number;readonly durationSeconds:number;readonly sampleRateHz?:number
 readonly cameraFromSensor?:readonly [number,number,number,number]
 readonly timeOffsetSeconds?:number;readonly accelerometerCorrection?:boolean
 readonly visualDriftCorrection?:boolean;readonly driftCorrection?:Insta360DriftCorrection
 readonly orientationToleranceDegrees?:number;readonly motionPaddingSeconds?:number;readonly gravityPrerollSeconds?:number
}
export interface Insta360GyroAnalysis {
 readonly status:'sensor_axes_required'|'orientation_ready'
 readonly mountingSource:'embedded_v6_145'|'embedded_legacy_145'|'explicit'|'unresolved'
 readonly synchronization:string;readonly timingWarning?:string|null;readonly limitations:readonly string[];readonly inputSamples:number
 readonly captureStartSeconds:number;readonly captureEndSeconds:number
 readonly orientationOffsetSeconds?:number
 readonly originalTimeMapOffsetSeconds?:number|null
 readonly gravityAlignment?:{readonly status:string;readonly warning:string|null;readonly requestedForceNormG:number;readonly observedPrerollSeconds:number;readonly initializationSamples:number}
 readonly captureClock?:readonly import('./composition/types').WireCaptureTime[]
 readonly sourceFrameClock?:import('./composition/types').WireSourceFrameClock|null
 readonly driftCorrection?:{readonly source:string;readonly visualFrames:number;readonly visualPairs:number;readonly visualRejectedPairs:number;readonly correctedHeadingDegrees:number;readonly warnings:readonly string[]}

 readonly samples:readonly {readonly timeSeconds:number;readonly accelerationG:readonly number[];readonly radiansPerSecond:readonly number[]}[]
 readonly orientation:readonly {readonly timeSeconds:number;readonly quaternion:readonly [number,number,number,number]}[]
}
export function analyzeInsta360Gyro(id:string,token:string,data:Insta360GyroRequest,signal?:AbortSignal):Promise<Insta360GyroAnalysis|null>{
 return requestJson(`/api/library/${encodeURIComponent(id)}/insta360/gyro`,{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:JSON.stringify(data),signal})
}

/** Saved project-owned SAM masks decoded and measured natively, including FFV1 transport. */
export function measureSphericalSamRegion(token:string,projectId:string,request:{compositionId?:string;clipId:string;expectedRevision:number;region:import('./composition/types').SamVideoRegionAttachment;sourceTimesUs:readonly number[];spherical?:import('./composition/types').WireSphericalVideo;parameterStartUs?:number;reidentify?:boolean;framing?:{aspectRatio:number;targetFill:number;initialFov:number}},signal?:AbortSignal):Promise<{centres:{pan:number;tilt:number;fov?:number}[];sampleIndices?:number[];states?:string[]}>{
 return requestJson(`/api/composition-projects/${encodeURIComponent(projectId)}/spherical-mask-track`,{method:'POST',headers:bearerHeaders(token,true),body:JSON.stringify(request),signal})
}

export type Insta360CameraProfile = import('./composition/types').WireInsta360CameraProfile
export interface Insta360ProfileAnalysis {
 readonly calibration:{readonly status:string;readonly renderReady:boolean;readonly reason:string;readonly calibrationField:number|null;readonly profile:Insta360CameraProfile|null}
 readonly sourceLayout:'dual_video_streams'|null;readonly packingReason:string
}
export function inspectInsta360Profile(id:string,token:string,signal?:AbortSignal):Promise<Insta360ProfileAnalysis|null>{
 return requestJson(`/api/library/${encodeURIComponent(id)}/insta360/profile`,{headers:{Authorization:`Bearer ${token}`},signal})
}

export interface Insta360DriftCorrection {
 readonly biasRadiansPerSecond?:readonly [number,number,number]
 readonly stationaryIntervals?:readonly {readonly startSeconds:number;readonly endSeconds:number}[]
 readonly headingAnchors?:readonly {readonly timeSeconds:number;readonly headingDegrees:number}[]
}
