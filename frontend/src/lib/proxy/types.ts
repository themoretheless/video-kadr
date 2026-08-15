import type { JobStatus } from '$lib/types.js'

export type ProxyCodec = 'h264' | 'prores_proxy'

export interface ProxyProfile {
  maxWidth: number
  codec: ProxyCodec
  quality: number
  includeAudio: boolean
}

export interface ProxyArtifact {
  key: string
  profile: ProxyProfile
  status: 'ready'
  url: string
  sizeBytes: number
  sha256: string
}

export interface ProxyJobSummary {
  jobId: string
  key: string
  profile: ProxyProfile
  status: Extract<JobStatus, 'pending' | 'running'>
  progress?: number
  stage?: string
}

export interface ProxyList {
  sourceId: string
  sourceFingerprint: string
  status: 'none' | 'processing' | 'ready'
  proxies: ProxyArtifact[]
  jobs: ProxyJobSummary[]
}

export interface ProxyCreateResult {
  jobId: string
  key: string
}

const FINGERPRINT = /^[0-9a-f]{64}$/
const JOB_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/
const SOURCE_ID_MAX_CHARS = 256
const MAX_ARTIFACTS = 64

function invalid(label: string): never {
  throw new TypeError(`Некорректный ответ proxy: ${label}`)
}

function record(
  value: unknown,
  label: string,
  required: readonly string[],
  optional: readonly string[] = [],
): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid(label)
  const result = value as Record<string, unknown>
  const allowed = new Set([...required, ...optional])
  if (required.some((key) => !Object.hasOwn(result, key))) invalid(label)
  if (Object.keys(result).some((key) => !allowed.has(key))) invalid(label)
  return result
}

function fingerprint(value: unknown, label: string): string {
  if (typeof value !== 'string' || !FINGERPRINT.test(value)) invalid(label)
  return value
}

function boundedString(value: unknown, label: string, maxChars: number): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > maxChars || /[\0\r\n]/.test(value)) {
    invalid(label)
  }
  return value
}

function finiteInteger(value: unknown, label: string, min: number, max: number): number {
  if (!Number.isSafeInteger(value) || (value as number) < min || (value as number) > max) invalid(label)
  return value as number
}

export function parseProxyProfile(value: unknown): ProxyProfile {
  const body = record(value, 'profile', ['maxWidth', 'codec', 'quality', 'includeAudio'])
  const codec = body.codec
  if (codec !== 'h264' && codec !== 'prores_proxy') invalid('profile.codec')
  if (typeof body.includeAudio !== 'boolean') invalid('profile.includeAudio')
  return {
    maxWidth: finiteInteger(body.maxWidth, 'profile.maxWidth', 160, 3840),
    codec,
    quality: finiteInteger(body.quality, 'profile.quality', 0, 63),
    includeAudio: body.includeAudio,
  }
}

export function isSafeProxyMediaUrl(value: string, key?: string, codec?: ProxyCodec): boolean {
  const match = /^\/files\/proxies\/([0-9a-f]{64})\.(mp4|mov)$/.exec(value)
  if (!match || (key !== undefined && match[1] !== key)) return false
  if (codec === 'h264' && match[2] !== 'mp4') return false
  if (codec === 'prores_proxy' && match[2] !== 'mov') return false
  return true
}

function parseArtifact(value: unknown): ProxyArtifact {
  const body = record(value, 'artifact', ['key', 'profile', 'status', 'url', 'sizeBytes', 'sha256'])
  const key = fingerprint(body.key, 'artifact.key')
  const profile = parseProxyProfile(body.profile)
  if (body.status !== 'ready') invalid('artifact.status')
  if (typeof body.url !== 'string' || !isSafeProxyMediaUrl(body.url, key, profile.codec)) {
    invalid('artifact.url')
  }
  return {
    key,
    profile,
    status: 'ready',
    url: body.url,
    sizeBytes: finiteInteger(body.sizeBytes, 'artifact.sizeBytes', 0, Number.MAX_SAFE_INTEGER),
    sha256: fingerprint(body.sha256, 'artifact.sha256'),
  }
}

function parseJob(value: unknown): ProxyJobSummary {
  const body = record(
    value,
    'job',
    ['jobId', 'key', 'profile', 'status'],
    ['progress', 'stage'],
  )
  const status = body.status
  if (status !== 'pending' && status !== 'running') invalid('job.status')
  const progress = body.progress
  if (progress !== undefined && (typeof progress !== 'number' || !Number.isFinite(progress) || progress < 0 || progress > 100)) {
    invalid('job.progress')
  }
  const stage = body.stage
  if (stage !== undefined && (typeof stage !== 'string' || stage.length > 128 || /[\0\r\n]/.test(stage))) {
    invalid('job.stage')
  }
  return {
    jobId: boundedString(body.jobId, 'job.jobId', 128),
    key: fingerprint(body.key, 'job.key'),
    profile: parseProxyProfile(body.profile),
    status,
    ...(progress === undefined ? {} : { progress }),
    ...(stage === undefined ? {} : { stage }),
  }
}

export function parseProxyCreateResult(value: unknown): ProxyCreateResult {
  const body = record(value, 'create result', ['jobId', 'key'])
  const jobId = boundedString(body.jobId, 'create result.jobId', 128)
  if (!JOB_ID.test(jobId)) invalid('create result.jobId')
  return { jobId, key: fingerprint(body.key, 'create result.key') }
}

export function parseProxyList(value: unknown, expectedSourceId?: string): ProxyList {
  const body = record(value, 'list', ['sourceId', 'sourceFingerprint', 'status', 'proxies', 'jobs'])
  const sourceId = boundedString(body.sourceId, 'list.sourceId', SOURCE_ID_MAX_CHARS)
  if (expectedSourceId !== undefined && sourceId !== expectedSourceId) invalid('list.sourceId')
  if (!Array.isArray(body.proxies) || body.proxies.length > MAX_ARTIFACTS) invalid('list.proxies')
  if (!Array.isArray(body.jobs) || body.jobs.length > MAX_ARTIFACTS) invalid('list.jobs')
  const proxies = body.proxies.map(parseArtifact)
  const jobs = body.jobs.map(parseJob)
  if (new Set(proxies.map(({ key }) => key)).size !== proxies.length) invalid('list.proxies duplicates')
  if (new Set(jobs.map(({ jobId }) => jobId)).size !== jobs.length) invalid('list.jobs duplicates')

  const status = body.status
  if (status !== 'none' && status !== 'processing' && status !== 'ready') invalid('list.status')
  if (status === 'processing' && jobs.length === 0) invalid('list.status')
  if (status === 'ready' && (jobs.length !== 0 || proxies.length === 0)) invalid('list.status')
  if (status === 'none' && (jobs.length !== 0 || proxies.length !== 0)) invalid('list.status')

  return {
    sourceId,
    sourceFingerprint: fingerprint(body.sourceFingerprint, 'list.sourceFingerprint'),
    status,
    proxies,
    jobs,
  }
}

export function requireProxyKey(value: string): string {
  return fingerprint(value, 'key')
}
