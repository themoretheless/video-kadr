import type {
  Capabilities,
  Job,
  LutAsset,
  MediaEntry,
  ResultInfo,
  VideoInfo,
} from './types'
import type { ProjectDto } from './api'
import { allProjects, compareAndSwapProject, projectById, projectByVideo, removeProject } from './browser-project-store'
import { createProjectDocumentFromLegacy, migrateProjectDocument } from './project-schema'
import type { ProjectMedia } from './project-schema'
import {
  auditBrowserAssets,
  BrowserAssetStorageError,
  deleteBrowserAsset,
  getBrowserAsset,
  fingerprintBlob,
  putBrowserAsset,
  prepareBrowserStorage,
  requestExternalHandleAccess,
  relinkBrowserAsset,
} from './browser-asset-store'
import type { BrowserStorageEstimate } from './browser-asset-store'
import { boundedEnginePhase, BrowserEngineTimeoutError } from './browser-engine-guard'
import {
  isLikelyOutOfMemory,
  assertBoundedBrowserOutput,
  assertCompleteMediaDuration,
  browserMemoryBudget,
  MEMFS_FALLBACK_MAX_INPUT_BYTES,
  MEMFS_MAX_OUTPUT_BYTES,
  OPFS_INGEST_OVERHEAD_BYTES,
  planBrowserImport,
  planBrowserExport,
  runtimeResourceCapabilities,
} from './browser-resource-plan'

type EditPayload = Record<string, unknown>

interface SourceRecord {
  file: File
  info: VideoInfo
}

interface LutRecord {
  file: File
  asset: LutAsset
}

interface BrowserJob extends Job {
  cancelled?: boolean
}

let fallbackProjectAssetLock = Promise.resolve()
let fallbackRenderLock = Promise.resolve()

async function withProjectAssetLock<T>(operation: () => Promise<T>): Promise<T> {
  if (navigator.locks) {
    return navigator.locks.request('video-kadr-project-assets', { mode: 'exclusive' }, operation)
  }
  const previous = fallbackProjectAssetLock
  let release!: () => void
  fallbackProjectAssetLock = new Promise<void>((resolve) => { release = resolve })
  await previous
  try {
    return await operation()
  } finally {
    release()
  }
}

async function withBrowserRenderLock<T>(operation: () => Promise<T>): Promise<T> {
  if (navigator.locks) {
    return navigator.locks.request('video-kadr-browser-render', { mode: 'exclusive' }, operation)
  }
  const previous = fallbackRenderLock
  let release!: () => void
  fallbackRenderLock = new Promise<void>((resolve) => { release = resolve })
  await previous
  try {
    return await operation()
  } finally {
    release()
  }
}

const sources = new Map<string, SourceRecord>()
const luts = new Map<string, LutRecord>()
const jobs = new Map<string, BrowserJob>()
const library: MediaEntry[] = []
const projects = new Map<string, ProjectDto>()
const MAX_SESSION_OUTPUT_BYTES = 256 * 1024 * 1024

let ffmpegInstance: import('@ffmpeg/ffmpeg').FFmpeg | null = null
let ffmpegLoading: Promise<import('@ffmpeg/ffmpeg').FFmpeg> | null = null
let activeJobId: string | null = null
let streamingExportActive = false
let cancelStreamingExport: (() => void) | null = null
let lastStorageEstimate: BrowserStorageEstimate | null = null

export class LinkImportRequiresServerError extends Error {
  constructor() {
    super('Импорт по ссылке будет доступен в полноценной версии сайта. В статической версии выберите файл с устройства.')
    this.name = 'LinkImportRequiresServerError'
  }
}

export class ProjectRevisionConflictError extends Error {
  constructor() {
    super('Проект уже изменён в другой вкладке. Перезагрузите последнюю версию перед сохранением.')
    this.name = 'ProjectRevisionConflictError'
  }
}

export function isBrowserProcessing(): boolean {
  return import.meta.env.VITE_PROCESSING_MODE === 'browser'
    || (typeof location !== 'undefined' && new URLSearchParams(location.search).get('processing') === 'browser')
}

function id(): string {
  return crypto.randomUUID()
}

function objectUrl(blob: Blob): string {
  return URL.createObjectURL(blob)
}

function pruneSessionOutputs(): void {
  const outputs = library.filter((entry) => entry.kind === 'output')
  let bytes = outputs.reduce((total, entry) => total + (entry.sizeBytes ?? 0), 0)
  for (const entry of [...outputs].reverse()) {
    if (bytes <= MAX_SESSION_OUTPUT_BYTES) break
    bytes -= entry.sizeBytes ?? 0
    URL.revokeObjectURL(entry.url)
    const index = library.findIndex((candidate) => candidate.id === entry.id)
    if (index >= 0) library.splice(index, 1)
  }
}

function probeVideo(file: File): Promise<Omit<VideoInfo, 'id' | 'url' | 'filename'>> {
  return new Promise((resolve, reject) => {
    const url = objectUrl(file)
    const video = document.createElement('video')
    video.preload = 'metadata'
    video.onloadedmetadata = () => {
      const duration = Number.isFinite(video.duration) ? video.duration : 0
      const width = video.videoWidth
      const height = video.videoHeight
      URL.revokeObjectURL(url)
      if (!duration || !width || !height) {
        reject(new Error('Не удалось прочитать параметры видео'))
        return
      }
      resolve({ duration, width, height, title: file.name, sizeBytes: file.size })
    }
    video.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error('Браузер не смог открыть этот видеофайл'))
    }
    video.src = url
  })
}

async function probeAudio(file: File): Promise<Omit<VideoInfo, 'id' | 'url' | 'filename'>> {
  if (file.type === 'audio/wav' || /\.wav$/i.test(file.name)) {
    const header = await file.slice(0, 44).arrayBuffer()
    if (header.byteLength >= 44) {
      const bytes = new Uint8Array(header)
      const ascii = (start: number, length: number) =>
        String.fromCharCode(...bytes.slice(start, start + length))
      const view = new DataView(header)
      const byteRate = view.getUint32(28, true)
      const dataBytes = view.getUint32(40, true)
      if (
        ascii(0, 4) === 'RIFF'
        && ascii(8, 4) === 'WAVE'
        && ascii(12, 4) === 'fmt '
        && view.getUint32(16, true) === 16
        && ascii(36, 4) === 'data'
        && byteRate > 0
        && dataBytes > 0
      ) {
        return {
          duration: dataBytes / byteRate,
          width: 0,
          height: 0,
          title: file.name,
          sizeBytes: file.size,
        }
      }
    }
  }
  return new Promise((resolve, reject) => {
    const url = objectUrl(file)
    const audio = document.createElement('audio')
    audio.preload = 'metadata'
    audio.onloadedmetadata = () => {
      const duration = Number.isFinite(audio.duration) ? audio.duration : 0
      URL.revokeObjectURL(url)
      if (!duration) {
        reject(new Error('Не удалось прочитать параметры аудио'))
        return
      }
      resolve({ duration, width: 0, height: 0, title: file.name, sizeBytes: file.size })
    }
    audio.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error('Браузер не смог открыть этот аудиофайл'))
    }
    audio.src = url
  })
}

export async function uploadFile(file: File): Promise<VideoInfo> {
  const isVideo = file.type.startsWith('video/')
  const isAudio = file.type.startsWith('audio/')
  if (!isVideo && !isAudio) throw new Error('Выберите видео- или аудиофайл')
  let durableStorage = true
  try {
    // OPFS publication temporarily holds staging + immutable bytes. Reserve
    // both copies and fixed manifest/journal headroom before mutating storage.
    const rawEstimate: StorageEstimate = await navigator.storage?.estimate?.().catch(() => ({} as StorageEstimate)) ?? {}
    const importPlan = planBrowserImport(file.size, rawEstimate ?? null)
    if (importPlan.risk === 'blocked') {
      lastStorageEstimate = {
        persisted: false,
        usage: rawEstimate?.usage ?? null,
        quota: rawEstimate?.quota ?? null,
        requiredBytes: importPlan.transientStorageBytes,
        availableBytes: importPlan.freeStorageBytes,
        risk: importPlan.risk,
        reason: importPlan.reason,
      }
      throw new BrowserAssetStorageError(importPlan.reason!, 'quota')
    }
    const storage = await prepareBrowserStorage(file.size * 2 + OPFS_INGEST_OVERHEAD_BYTES)
    lastStorageEstimate = {
      ...storage,
      requiredBytes: importPlan.transientStorageBytes,
      availableBytes: importPlan.freeStorageBytes,
      risk: importPlan.risk,
      reason: importPlan.reason,
    }
  } catch (error) {
    if (!(error instanceof BrowserAssetStorageError)) throw error
    durableStorage = false
    lastStorageEstimate ??= { persisted: false, usage: null, quota: null }
  }
  const metadata = isVideo ? await probeVideo(file) : await probeAudio(file)
  const sourceId = id()
  const info: VideoInfo = {
    id: sourceId,
    url: objectUrl(file),
    filename: file.name,
    mediaKind: isVideo ? 'video' : 'audio',
    ...metadata,
  }
  const createdAt = Date.now()
  const durableInfo = { ...info }
  delete (durableInfo as Partial<VideoInfo>).url
  try {
    if (!durableStorage) throw new BrowserAssetStorageError('Durable storage unavailable', 'unavailable')
    const manifest = await putBrowserAsset({
      id: sourceId,
      file,
      filename: file.name,
      fileType: file.type,
      info: durableInfo as Omit<VideoInfo, 'url'>,
      createdAt,
    })
    info.assetId = manifest.id
    info.fingerprint = manifest.fingerprint
    info.availability = 'ready'
  } catch (error) {
    if (!(error instanceof BrowserAssetStorageError) || error.reason === 'integrity') {
      URL.revokeObjectURL(info.url)
      throw error
    }
    info.assetId = sourceId
    info.fingerprint = await fingerprintBlob(file)
    info.availability = 'session'
  }
  sources.set(sourceId, { file, info })
  library.unshift({
    id: sourceId,
    kind: 'source',
    filename: file.name,
    url: info.url,
    title: file.name,
    duration: info.duration,
    width: info.width,
    height: info.height,
    fps: info.fps,
    vcodec: info.vcodec,
    acodec: info.acodec,
    mediaKind: info.mediaKind,
    assetId: info.assetId,
    fingerprint: info.fingerprint,
    sizeBytes: file.size,
    availability: info.availability,
    createdAt,
  })
  return info
}

function parseCubeSize(text: string): number {
  const match = text.match(/^\s*LUT_3D_SIZE\s+(\d+)\s*$/im)
  const size = match ? Number(match[1]) : 0
  if (!Number.isInteger(size) || size < 2 || size > 65) {
    throw new Error('LUT должен содержать LUT_3D_SIZE от 2 до 65')
  }
  return size
}

export async function uploadLut(file: File): Promise<LutAsset> {
  const cubeSize = parseCubeSize(await file.text())
  const lutId = id()
  const asset: LutAsset = { id: lutId, name: file.name, cubeSize, sizeBytes: file.size }
  luts.set(lutId, { file, asset })
  return asset
}

export function getLut(lutId: string): LutAsset {
  const record = luts.get(lutId)
  if (!record) throw new Error('LUT больше недоступен — загрузите файл повторно')
  return record.asset
}

function coreUrl(filename: string): string {
  return new URL(`ffmpeg-core/${filename}`, document.baseURI).href
}

async function loadFfmpeg(): Promise<import('@ffmpeg/ffmpeg').FFmpeg> {
  if (ffmpegInstance?.loaded) return ffmpegInstance
  if (ffmpegLoading) return ffmpegLoading
  ffmpegLoading = (async () => {
    const { FFmpeg } = await import('@ffmpeg/ffmpeg')
    const ffmpeg = new FFmpeg()
    try {
      await boundedEnginePhase(ffmpeg.load({
        coreURL: coreUrl('ffmpeg-core.js'),
        wasmURL: coreUrl('ffmpeg-core.wasm'),
      }), 'загрузка движка', 45_000, () => ffmpeg.terminate())
    } catch (error) {
      ffmpeg.terminate()
      throw error
    }
    ffmpegInstance = ffmpeg
    return ffmpeg
  })()
  try {
    return await ffmpegLoading
  } finally {
    ffmpegLoading = null
  }
}

function number(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function escapeFilterPath(path: string): string {
  return path.replace(/([\\':,;[\]])/g, '\\$1')
}

function curvePoints(value: unknown): string {
  if (!Array.isArray(value)) return '0/0 1/1'
  return value
    .map(record)
    .filter((point): point is Record<string, unknown> => Boolean(point))
    .map((point) => {
      const normalize = (coordinate: unknown) => {
        const value = number(coordinate)
        return Math.max(0, Math.min(1, value > 1 ? value / 255 : value))
      }
      return `${normalize(point.x)}/${normalize(point.y)}`
    })
    .join(' ')
}

function presetFilter(name: string): string | null {
  const presets: Record<string, string> = {
    grayscale: 'hue=s=0',
    sepia: 'colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131',
    warm: 'colorbalance=rs=.08:bs=-.06',
    cold: 'colorbalance=rs=-.06:bs=.08',
    'teal-orange': 'colorbalance=rs=.05:gs=-.02:bs=.05',
    faded: 'eq=contrast=.85:brightness=.04:saturation=.9',
    noir: 'hue=s=0,eq=contrast=1.4',
    vintage: 'curves=vintage',
  }
  return presets[name] ?? null
}

function outputSpec(payload: EditPayload): { filename: string; mime: string; args: string[] } {
  const format = String(payload.format || 'mp4')
  const quality = number(payload.quality, 23)
  switch (format) {
    case 'webm':
      return { filename: 'edited.webm', mime: 'video/webm', args: ['-c:v', 'libvpx-vp9', '-crf', String(quality), '-b:v', '0', '-maxrate', '8M', '-bufsize', '16M', '-c:a', 'libopus'] }
    case 'gif':
      return { filename: 'edited.gif', mime: 'image/gif', args: ['-an', '-loop', '0'] }
    case 'png':
      return { filename: 'frame.png', mime: 'image/png', args: ['-frames:v', '1', '-an'] }
    case 'jpg':
      return { filename: 'frame.jpg', mime: 'image/jpeg', args: ['-frames:v', '1', '-q:v', '2', '-an'] }
    case 'mp3':
      return { filename: 'audio.mp3', mime: 'audio/mpeg', args: ['-vn', '-c:a', 'libmp3lame', '-q:a', '2'] }
    default:
      return { filename: 'edited.mp4', mime: 'video/mp4', args: ['-c:v', 'libx264', '-preset', 'veryfast', '-crf', String(quality), '-maxrate', '8M', '-bufsize', '16M', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-movflags', '+faststart'] }
  }
}

interface FfmpegJobSpec {
  ffmpegArgs: string[]
  inputName: string
  filename: string
  mime: string
  mountPoint?: string
  temporaryFiles: string[]
}

interface FfmpegJobResources {
  mountPoint?: string
  temporaryFiles: string[]
}

async function buildArgs(
  ffmpeg: import('@ffmpeg/ffmpeg').FFmpeg,
  source: SourceRecord,
  payload: EditPayload,
  resources: FfmpegJobResources,
): Promise<FfmpegJobSpec> {
  const extension = source.file.name.split('.').pop()?.replace(/[^a-z0-9]/gi, '') || 'mp4'
  const temporaryFiles = resources.temporaryFiles
  let mountPoint = resources.mountPoint
  let inputName: string
  const runtime = runtimeResourceCapabilities()
  if (runtime.workerFs && typeof ffmpeg.mount === 'function') {
    const { FFFSType } = await import('@ffmpeg/ffmpeg')
    mountPoint = `/source-${id()}`
    resources.mountPoint = mountPoint
    await ffmpeg.createDir(mountPoint)
    try {
      const mounted = await boundedEnginePhase(ffmpeg.mount(FFFSType.WORKERFS, { files: [source.file] }, mountPoint), 'WORKERFS mount', 5_000, () => ffmpeg.terminate())
      if (mounted === false) throw new Error('WORKERFS mount отклонён движком')
      inputName = `${mountPoint}/${source.file.name}`
    } catch (error) {
      if (error instanceof BrowserEngineTimeoutError) throw error
      await ffmpeg.deleteDir(mountPoint).catch(() => undefined)
      resources.mountPoint = undefined
      mountPoint = undefined
      if (source.file.size > MEMFS_FALLBACK_MAX_INPUT_BYTES) {
        throw new Error(`WORKERFS недоступен, а файл превышает bounded MEMFS fallback: ${error instanceof Error ? error.message : String(error)}`)
      }
      const fallbackPlan = planBrowserExport(source.info, payload, { ...runtime, workerFs: false })
      if (fallbackPlan.risk === 'blocked') throw new Error(fallbackPlan.reason || 'Недостаточно памяти для MEMFS fallback')
      inputName = `input-${id()}.${extension}`
      temporaryFiles.push(inputName)
      await ffmpeg.writeFile(inputName, new Uint8Array(await source.file.arrayBuffer()))
    }
  } else {
    inputName = `input-${id()}.${extension}`
    temporaryFiles.push(inputName)
    await ffmpeg.writeFile(inputName, new Uint8Array(await source.file.arrayBuffer()))
  }

  const args: string[] = []
  const trim = record(payload.trim)
  if (trim) {
    args.push('-ss', String(number(trim.start)), '-to', String(number(trim.end)))
  }
  args.push('-i', inputName)

  const videoFilters: string[] = []
  const audioFilters: string[] = []
  const segments = Array.isArray(payload.segments) ? payload.segments.map(record).filter(Boolean) : []
  if (segments.length === 2) {
    const cutStart = number(segments[0]?.end)
    const cutEnd = number(segments[1]?.start)
    videoFilters.push(`select=not(between(t\\,${cutStart}\\,${cutEnd}))`, 'setpts=N/FRAME_RATE/TB')
    audioFilters.push(`aselect=not(between(t\\,${cutStart}\\,${cutEnd}))`, 'asetpts=N/SR/TB')
  }

  const crop = record(payload.crop)
  if (crop) videoFilters.push(`crop=${number(crop.w)}:${number(crop.h)}:${number(crop.x)}:${number(crop.y)}`)
  const scale = record(payload.scale)
  if (scale) videoFilters.push(`scale=${number(scale.w)}:${number(scale.h, -2)}`)
  switch (number(payload.rotate)) {
    case 90: videoFilters.push('transpose=1'); break
    case 180: videoFilters.push('hflip', 'vflip'); break
    case 270: videoFilters.push('transpose=2'); break
  }
  if (payload.flipH) videoFilters.push('hflip')
  if (payload.flipV) videoFilters.push('vflip')

  const speed = Math.max(0.5, Math.min(2, number(payload.speed, 1)))
  if (speed !== 1) {
    videoFilters.push(`setpts=PTS/${speed}`)
    audioFilters.push(`atempo=${speed}`)
  }
  const brightness = number(payload.brightness)
  const contrast = number(payload.contrast, 1)
  const saturation = number(payload.saturation, 1)
  if (brightness || contrast !== 1 || saturation !== 1) {
    videoFilters.push(`eq=brightness=${brightness}:contrast=${contrast}:saturation=${saturation}`)
  }
  const preset = presetFilter(String(payload.filter || ''))
  if (preset) videoFilters.push(preset)

  const lut = record(payload.lut)
  if (lut) {
    const lutRecord = luts.get(String(lut.id))
    if (!lutRecord) throw new Error('LUT больше недоступен — загрузите файл повторно')
    const lutName = `lut-${id()}.cube`
    await ffmpeg.writeFile(lutName, new Uint8Array(await lutRecord.file.arrayBuffer()))
    temporaryFiles.push(lutName)
    videoFilters.push(`lut3d=file='${escapeFilterPath(lutName)}'`)
  }
  const curves = record(payload.curves)
  if (curves) {
    videoFilters.push(
      `curves=interp=pchip:master='${curvePoints(curves.master)}':r='${curvePoints(curves.red)}':g='${curvePoints(curves.green)}':b='${curvePoints(curves.blue)}'`,
    )
  }
  if (payload.reverse) {
    videoFilters.push('reverse')
    audioFilters.push('areverse')
  }
  if (payload.fps) videoFilters.push(`fps=${number(payload.fps)}`)
  const censor = record(payload.censor)
  if (censor) {
    videoFilters.push(`drawbox=x=${number(censor.x)}:y=${number(censor.y)}:w=${number(censor.w)}:h=${number(censor.h)}:color=${String(payload.censorColor || 'black')}:t=fill`)
  }
  if (payload.vignette) videoFilters.push('vignette')
  if (payload.denoise) videoFilters.push('hqdn3d')
  if (number(payload.sharpen) > 0) videoFilters.push(`unsharp=5:5:${number(payload.sharpen)}`)
  if (number(payload.grain) > 0) videoFilters.push(`noise=alls=${Math.round(number(payload.grain) * 30)}:allf=t`)
  const pad = String(payload.pad || '')
  if (/^\d+:\d+$/.test(pad)) {
    const [rw, rh] = pad.split(':').map(Number)
    videoFilters.push(`pad=w=max(iw\\,ih*${rw}/${rh}):h=max(ih\\,iw*${rh}/${rw}):x=(ow-iw)/2:y=(oh-ih)/2:color=black`)
  }

  if (number(payload.volume, 1) !== 1) audioFilters.push(`volume=${number(payload.volume, 1)}`)
  if (payload.normalizeAudio) audioFilters.push('loudnorm')
  if (payload.highpass) audioFilters.push('highpass=f=100')
  if (number(payload.fadeIn) > 0) audioFilters.push(`afade=t=in:st=0:d=${number(payload.fadeIn)}`)
  if (number(payload.fadeOut) > 0) {
    const duration = trim ? number(trim.end) - number(trim.start) : source.info.duration
    const fade = Math.min(number(payload.fadeOut), duration)
    audioFilters.push(`afade=t=out:st=${Math.max(0, duration - fade)}:d=${fade}`)
  }

  if (videoFilters.length && String(payload.format || 'mp4') !== 'mp3') args.push('-vf', videoFilters.join(','))
  if (audioFilters.length && !payload.mute) args.push('-af', audioFilters.join(','))
  if (payload.mute) args.push('-an')
  const output = outputSpec(payload)
  args.push('-fs', String(MEMFS_MAX_OUTPUT_BYTES))
  args.push(...output.args, output.filename)
  temporaryFiles.push(output.filename)
  return { ffmpegArgs: args, inputName, filename: output.filename, mime: output.mime, mountPoint, temporaryFiles }
}

async function runJobUnlocked(jobId: string, payload: EditPayload): Promise<void> {
  const job = jobs.get(jobId)
  if (!job) return
  const sourceId = String(payload.videoId)
  const source = sources.get(sourceId) ?? await resolveSourceRecord(sourceId).catch(() => null)
  if (!source) {
    Object.assign(job, { status: 'error', error: 'Исходный файл больше недоступен — выберите его повторно' })
    return
  }
  let ffmpeg: import('@ffmpeg/ffmpeg').FFmpeg | null = null
  let spec: FfmpegJobSpec | null = null
  const resources: FfmpegJobResources = { temporaryFiles: [] }
  let onProgress: ((event: { progress: number }) => void) | null = null
  try {
    job.status = 'running'
    job.stage = 'Загружаю FFmpeg в браузер…'
    job.progress = 1
    ffmpeg = await loadFfmpeg()
    if (job.cancelled) throw new Error('cancelled')
    activeJobId = jobId
    onProgress = ({ progress }: { progress: number }) => {
      job.progress = Math.max(2, Math.min(99, Math.round(progress * 100)))
      job.stage = 'Обрабатываю на этом устройстве…'
    }
    ffmpeg.on('progress', onProgress)
    job.stage = `Подготавливаю исходник ${Math.ceil(source.file.size / (1024 * 1024))} МБ…`
    spec = await boundedEnginePhase(buildArgs(ffmpeg, source, payload, resources), 'подготовка файлов движка', 60_000, () => ffmpeg?.terminate())
    job.stage = 'Кодирую на этом устройстве…'
    const exitCode = await boundedEnginePhase(ffmpeg.exec(spec.ffmpegArgs), 'кодирование', 30 * 60_000, () => ffmpeg?.terminate())
    ffmpeg.off('progress', onProgress)
    if (job.cancelled) throw new Error('cancelled')
    if (exitCode !== 0) throw new Error(`FFmpeg завершился с кодом ${exitCode}`)
    if (['mp4', 'webm', 'gif', 'mp3'].includes(String(payload.format || 'mp4'))) {
      const validationName = `validation-${id()}.txt`
      resources.temporaryFiles.push(validationName)
      const validationExit = await boundedEnginePhase(ffmpeg.exec([
        '-v', 'error', '-i', spec.filename, '-map', '0', '-c', 'copy',
        '-progress', validationName, '-f', 'null', '-',
      ]), 'проверка результата', 30_000, () => ffmpeg?.terminate())
      if (validationExit !== 0) throw new Error('FFmpeg не смог проверить целостность результата')
      const validationData = await boundedEnginePhase(ffmpeg.readFile(validationName, 'utf8'), 'чтение проверки результата', 10_000, () => ffmpeg?.terminate())
      const validationText = typeof validationData === 'string' ? validationData : new TextDecoder().decode(validationData)
      const outTime = [...validationText.matchAll(/^out_time_us=(\d+)$/gm)].at(-1)?.[1]
      const duration = outTime ? Number(outTime) / 1_000_000 : Number.NaN
      const expected = planBrowserExport(source.info, payload)
      const tolerance = String(payload.format || 'mp4') === 'mp3'
        ? 0.1
        : 1 / Math.max(1, number(payload.fps, source.info.fps ?? 30))
      assertCompleteMediaDuration(duration, expected.estimatedOutputSeconds, tolerance)
    }
    const data = await boundedEnginePhase(ffmpeg.readFile(spec.filename), 'чтение результата', 30_000, () => ffmpeg?.terminate())
    const bytes = data instanceof Uint8Array ? data : new TextEncoder().encode(data)
    assertBoundedBrowserOutput(bytes.byteLength)
    const blobPart = bytes.buffer instanceof ArrayBuffer && bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength
      ? bytes.buffer
      : bytes.slice().buffer
    const blob = new Blob([blobPart], { type: spec.mime })
    const resultId = id()
    const result: ResultInfo = {
      id: resultId,
      url: objectUrl(blob),
      filename: spec.filename,
      sizeBytes: blob.size,
    }
    library.unshift({ id: resultId, kind: 'output', filename: spec.filename, url: result.url, sizeBytes: blob.size, createdAt: Date.now() })
    pruneSessionOutputs()
    Object.assign(job, { status: 'done', progress: 100, stage: 'Готово', result })
  } catch (error) {
    const cancelled = job.cancelled || (error instanceof Error && error.message === 'cancelled')
    const outOfMemory = isLikelyOutOfMemory(error)
    // Any engine-side failure may leave an unresolved message channel or a
    // high-water Wasm heap. Terminate first; never await cleanup on a poisoned
    // worker. The project/source live outside this disposable runtime.
    if (ffmpeg) ffmpeg.terminate()
    if (ffmpegInstance === ffmpeg) ffmpegInstance = null
    Object.assign(job, cancelled
      ? { status: 'cancelled', error: undefined }
      : { status: 'error', error: outOfMemory
        ? 'Недостаточно памяти для локального экспорта. Сократите диапазон или разрешение; движок сброшен, можно повторить экспорт.'
        : error instanceof Error ? error.message : String(error) })
  } finally {
    if (ffmpeg?.loaded && onProgress) ffmpeg.off('progress', onProgress)
    job.stage = job.status === 'done' ? 'Готово' : job.stage
    // A fresh disposable core per job is the bounded cleanup mechanism: it
    // releases MEMFS, mounts and the non-shrinking Wasm heap synchronously.
    if (ffmpeg?.loaded) ffmpeg.terminate()
    if (ffmpegInstance === ffmpeg) ffmpegInstance = null
    if (activeJobId === jobId) activeJobId = null
  }
}

async function runJob(jobId: string, payload: EditPayload): Promise<void> {
  // Web Locks serializes Wasm heaps across same-origin tabs. The local queue is
  // the deterministic fallback for engines without the API.
  return withBrowserRenderLock(() => runJobUnlocked(jobId, payload))
}

export function edit(payload: EditPayload): { jobId: string } {
  if (activeJobId) throw new Error('Дождитесь завершения текущего экспорта')
  const source = sources.get(String(payload.videoId))
  if (!source) throw new Error('Исходный файл больше недоступен — выберите его повторно')
  const plan = planBrowserExport(source.info, payload)
  if (plan.risk === 'blocked') throw new Error(`${plan.reason} ${plan.suggestions.join(' · ')}`)
  const jobId = id()
  jobs.set(jobId, { id: jobId, status: 'pending', progress: 0, stage: 'Подготовка…' })
  activeJobId = jobId
  void runJob(jobId, payload)
  return { jobId }
}

export function getJob(jobId: string): Job {
  const job = jobs.get(jobId)
  if (!job) throw new Error('Задача не найдена')
  return { ...job }
}

export function cancelJob(jobId: string): void {
  const job = jobs.get(jobId)
  if (!job || ['done', 'error', 'cancelled'].includes(job.status)) return
  job.cancelled = true
  job.status = 'cancelled'
  if (activeJobId === jobId && ffmpegInstance) {
    ffmpegInstance.terminate()
    ffmpegInstance = null
  }
}

type CaptureMediaElement = HTMLVideoElement & {
  captureStream?: () => MediaStream
  mozCaptureStream?: () => MediaStream
}

type SavePickerWindow = Window & {
  showSaveFilePicker?: (options?: {
    suggestedName?: string
    types?: Array<{ description?: string; accept: Record<string, string[]> }>
  }) => Promise<FileSystemFileHandle>
}

export function streamingOutputSupported(): boolean {
  const prototype = typeof HTMLVideoElement === 'undefined' ? null : HTMLVideoElement.prototype as CaptureMediaElement
  return Boolean(
    (window as SavePickerWindow).showSaveFilePicker
    && typeof MediaRecorder !== 'undefined'
    && (prototype?.captureStream || prototype?.mozCaptureStream),
  )
}

const STREAM_WRITE_TIMEOUT_MS = 15_000
const STREAM_MAX_PENDING_BYTES = 4 * 1024 * 1024

function withTimeout<T>(promise: Promise<T>, milliseconds: number, message: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => window.setTimeout(() => reject(new Error(message)), milliseconds)),
  ])
}

async function validateStreamedFile(file: File, expectedSeconds: number): Promise<void> {
  if (!file.size) throw new Error('Браузер создал пустой потоковый файл')
  const signature = new Uint8Array(await file.slice(0, 4).arrayBuffer())
  if (signature.length < 4 || signature[0] !== 0x1a || signature[1] !== 0x45 || signature[2] !== 0xdf || signature[3] !== 0xa3) {
    throw new Error('Сохранённый файл не является корректным контейнером WebM')
  }
  const probe = document.createElement('video')
  const url = objectUrl(file)
  try {
    probe.preload = 'metadata'
    probe.src = url
    await withTimeout(new Promise<void>((resolve, reject) => {
      probe.onloadedmetadata = () => resolve()
      probe.onerror = () => reject(new Error('Не удалось проверить сохранённый WebM'))
    }), 10_000, 'Проверка сохранённого WebM превысила лимит времени')
    // MediaRecorder WebM commonly omits a seekable duration. A very large seek
    // makes the media engine scan clusters and recover the last decodable time;
    // unlike loadedmetadata this fails closed for header-only/truncated output.
    if (!Number.isFinite(probe.duration)) {
      probe.currentTime = Number.MAX_SAFE_INTEGER
      await withTimeout(new Promise<void>((resolve, reject) => {
        probe.onseeked = () => resolve()
        probe.onerror = () => reject(new Error('Сохранённый WebM обрывается до финального media cluster'))
      }), 10_000, 'Не удалось декодировать финальный media cluster WebM')
    }
    const decodedDuration = Number.isFinite(probe.duration) ? probe.duration : probe.currentTime
    if (!Number.isFinite(decodedDuration) || decodedDuration <= 0) {
      throw new Error('Не удалось определить декодированную длительность WebM')
    }
    assertCompleteMediaDuration(decodedDuration, expectedSeconds, 0.2)
  } finally {
    probe.removeAttribute('src')
    probe.load()
    URL.revokeObjectURL(url)
  }
}

/**
 * True streaming fallback for GitHub Pages: capture the original selected
 * range and write MediaRecorder chunks straight to a user-chosen file. This
 * deliberately does not claim to render filters; it is the bounded recovery
 * route when a full FFmpeg result would exceed MEMFS.
 */
export async function streamOriginalRange(payload: EditPayload): Promise<ResultInfo> {
  if (!streamingOutputSupported()) throw new Error('Потоковое сохранение недоступно в этом браузере')
  if (streamingExportActive || activeJobId) throw new Error('Другой экспорт уже выполняется')
  const source = sources.get(String(payload.videoId))
  if (!source) throw new Error('Исходный файл больше недоступен — выберите его повторно')
  streamingExportActive = true
  const picker = (window as SavePickerWindow).showSaveFilePicker!
  const handle = await picker({
    suggestedName: source.file.name.replace(/\.[^.]+$/, '') + '-stream.webm',
    types: [{ description: 'WebM video', accept: { 'video/webm': ['.webm'] } }],
  }).catch((error) => {
    streamingExportActive = false
    throw error
  })
  const writable = await handle.createWritable().catch((error) => {
    streamingExportActive = false
    throw error
  })
  const media = document.createElement('video') as CaptureMediaElement
  media.playsInline = true
  media.preload = 'auto'
  media.src = objectUrl(source.file)
  const trim = record(payload.trim)
  const start = Math.max(0, number(trim?.start))
  const end = Math.min(source.info.duration, number(trim?.end, source.info.duration))
  let writeQueue: Promise<void> = Promise.resolve()
  let watchdog: number | undefined
  let cancelled = false
  let recorder: MediaRecorder | null = null
  let stream: MediaStream | null = null
  let pendingBytes = 0
  let writeError: unknown = null
  try {
    await new Promise<void>((resolve, reject) => {
      media.onloadedmetadata = () => resolve()
      media.onerror = () => reject(new Error('Браузер не смог подготовить исходник для потокового сохранения'))
    })
    media.currentTime = start
    if (start > 0) await new Promise<void>((resolve) => { media.onseeked = () => resolve() })
    const capture = media.captureStream ?? media.mozCaptureStream
    stream = capture!.call(media)
    const mime = MediaRecorder.isTypeSupported('video/webm;codecs=vp8,opus')
      ? 'video/webm;codecs=vp8,opus'
      : 'video/webm'
    recorder = new MediaRecorder(stream, { mimeType: mime })
    const stopRecorder = () => {
      if (recorder && recorder.state !== 'inactive') recorder.stop()
    }
    cancelStreamingExport = () => {
      cancelled = true
      media.pause()
      stopRecorder()
    }
    const stopped = new Promise<void>((resolve, reject) => {
      recorder!.ondataavailable = (event) => {
        if (!event.data.size) return
        pendingBytes += event.data.size
        media.pause()
        if (recorder?.state === 'recording') recorder.pause()
        if (pendingBytes > STREAM_MAX_PENDING_BYTES) {
          writeError = new Error('Браузер не успевает записывать поток на диск')
          cancelled = true
          stopRecorder()
          return
        }
        writeQueue = writeQueue.then(async () => {
          await withTimeout(Promise.resolve(writable.write(event.data)), STREAM_WRITE_TIMEOUT_MS, 'Запись потокового файла зависла')
          pendingBytes -= event.data.size
          if (!cancelled && recorder?.state === 'paused') {
            recorder.resume()
            await media.play()
          }
        }).catch((error) => {
          writeError = error
          cancelled = true
          stopRecorder()
          throw error
        })
      }
      recorder!.onerror = () => reject(new Error('MediaRecorder завершился с ошибкой'))
      recorder!.onstop = () => resolve()
    })
    recorder.start(500)
    const finishAtMediaTime = () => {
      if (media.currentTime + 0.03 < end) return
      media.pause()
      stopRecorder()
    }
    media.ontimeupdate = finishAtMediaTime
    media.onended = finishAtMediaTime
    await media.play()
    // This is a failure watchdog only. Normal completion is driven by media time,
    // which remains correct when a hidden tab throttles timers or playback.
    watchdog = window.setTimeout(() => {
      writeError = new Error('Потоковое воспроизведение не продвигается')
      cancelled = true
      media.pause()
      stopRecorder()
    }, Math.max(15_000, (end - start) * 3_000 + 15_000))
    await stopped
    await writeQueue.catch(() => undefined)
    if (writeError) throw writeError
    if (cancelled) throw new DOMException('Потоковый экспорт отменён', 'AbortError')
    await writable.close()
    const file = await handle.getFile()
    await validateStreamedFile(file, end - start)
    const result: ResultInfo = { id: id(), url: objectUrl(file), filename: handle.name, sizeBytes: file.size }
    library.unshift({ ...result, kind: 'output', createdAt: Date.now() })
    pruneSessionOutputs()
    return result
  } catch (error) {
    await writeQueue.catch(() => undefined)
    await writable.abort().catch(() => undefined)
    throw error
  } finally {
    if (watchdog !== undefined) window.clearTimeout(watchdog)
    cancelled = true
    media.pause()
    media.ontimeupdate = null
    media.onended = null
    if (recorder && recorder.state !== 'inactive') recorder.stop()
    stream?.getTracks().forEach((track) => track.stop())
    URL.revokeObjectURL(media.src)
    media.removeAttribute('src')
    media.load()
    streamingExportActive = false
    cancelStreamingExport = null
  }
}

export function cancelStreamingOutput(): void {
  cancelStreamingExport?.()
}

export function getCapabilities(): Capabilities {
  const enabled = (id: string, label = id) => ({ id, label, available: true })
  const disabled = (id: string, label: string, reason: string) => ({ id, label, available: false, reason })
  const runtime = runtimeResourceCapabilities()
  const runtimeReason = !runtime.wasm
    ? 'WebAssembly недоступен в этом браузере'
    : !runtime.worker ? 'Web Worker недоступен в этом браузере' : null
  const local = (id: string, label = id) => runtimeReason ? disabled(id, label, runtimeReason) : enabled(id, label)
  return {
    schemaVersion: 1,
    toolFingerprint: 'ffmpeg.wasm/client',
    formats: [
      local('mp4', 'MP4'), local('webm', 'WebM'), local('gif', 'GIF'),
      local('png', 'PNG'), local('jpg', 'JPG'), local('mp3', 'MP3'),
      disabled('av1', 'AV1', 'Недоступно в браузерной сборке'),
      disabled('prores', 'ProRes', 'Недоступно в браузерной сборке'),
    ],
    codecs: [
      local('h264', 'H.264'),
      disabled('h265', 'H.265', 'Недоступно в браузерной сборке'),
    ],
    filters: [
      ...['grayscale', 'sepia', 'warm', 'cold', 'teal-orange', 'faded', 'noir', 'vintage', 'lut3d', 'curves'].map((name) => enabled(name)),
      disabled('lut3d-blend', 'Интенсивность LUT', 'В браузере LUT применяется с интенсивностью 100%'),
    ],
    hardware: [disabled('native', 'Аппаратное ускорение', 'Используется WebAssembly')],
    runtime: {
      worker: runtime.worker,
      wasm: runtime.wasm,
      workerFs: runtime.workerFs,
      opfs: runtime.opfs,
      webCrypto: runtime.webCrypto,
      streamingOutput: streamingOutputSupported(),
      memoryBudgetBytes: browserMemoryBudget(runtime),
    },
  }
}

export function getStorageStatus(): BrowserStorageEstimate | null {
  return lastStorageEstimate
}

export interface BrowserArchiveSource {
  file: Blob
  filename: string
  fileType: string
  fingerprint: string
  info: Omit<VideoInfo, 'url'>
}

/** Read exact source bytes for a portable archive. Session-only sources are
 * intentionally supported while the tab still owns them; durable sources are
 * verified again by the asset store before being returned. */
export async function readSourceForArchive(
  sourceId: string,
  expectedFingerprint: string,
): Promise<BrowserArchiveSource> {
  const cached = sources.get(sourceId)
  if (cached) {
    const fingerprint = await fingerprintBlob(cached.file)
    if (fingerprint !== expectedFingerprint) {
      throw new BrowserAssetStorageError('Исходник изменён и не совпадает с проектом.', 'fingerprint')
    }
    const info = Object.fromEntries(
      Object.entries(cached.info).filter(([key]) => key !== 'url'),
    ) as Omit<VideoInfo, 'url'>
    return {
      file: cached.file,
      filename: cached.file.name || cached.info.filename,
      fileType: cached.file.type,
      fingerprint,
      info,
    }
  }
  const asset = await getBrowserAsset(sourceId)
  if (!(asset.file instanceof Blob) || asset.fingerprint !== expectedFingerprint) {
    throw new BrowserAssetStorageError('Исходник отсутствует или не совпадает с проектом.', 'fingerprint')
  }
  return {
    file: asset.file,
    filename: asset.filename,
    fileType: asset.fileType,
    fingerprint: asset.fingerprint,
    info: asset.info,
  }
}

export async function getLibrary(): Promise<MediaEntry[]> {
  let persisted
  try {
    persisted = await auditBrowserAssets()
  } catch (error) {
    if (error instanceof BrowserAssetStorageError) {
      return [...library].sort((left, right) => right.createdAt - left.createdAt)
    }
    throw error
  }
  const savedProjects = await allProjects()
  const expectedFingerprints = new Map<string, Set<string>>()
  const expectedProjectMedia = new Map<string, ProjectMedia>()
  for (const project of savedProjects) for (const media of project.document?.media ?? []) {
    const assetRef = media.assetRef ?? media.id
    if (!expectedProjectMedia.has(assetRef)) expectedProjectMedia.set(assetRef, media)
    if (!media.contentFingerprint) continue
    const values = expectedFingerprints.get(assetRef) ?? new Set<string>()
    values.add(media.contentFingerprint)
    expectedFingerprints.set(assetRef, values)
  }
  for (const asset of persisted) {
    const expected = expectedFingerprints.get(asset.id)
    const projectMedia = expectedProjectMedia.get(asset.id)
    const fingerprintConflict = Boolean(expected && (expected.size > 1 || !expected.has(asset.fingerprint)))
    const projectFingerprint = expected?.size === 1 ? [...expected][0] : undefined
    const projectMetadata = projectMedia?.metadata
    const projectSize = typeof projectMetadata?.sizeBytes === 'number' && Number.isFinite(projectMetadata.sizeBytes)
      ? projectMetadata.sizeBytes
      : undefined
    const existing = library.find((entry) => entry.id === asset.id)
    const info = sources.get(asset.id)?.info
    const entry: MediaEntry = {
      ...asset.info,
      kind: 'source',
      url: fingerprintConflict ? '' : info?.url ?? '',
      availability: fingerprintConflict ? 'offline' : asset.availability,
      assetId: asset.id,
      fingerprint: fingerprintConflict ? projectFingerprint : asset.fingerprint,
      identityConflict: Boolean(expected && expected.size > 1),
      sizeBytes: fingerprintConflict ? projectSize : asset.info.sizeBytes,
      createdAt: asset.createdAt,
    }
    if (existing) Object.assign(existing, entry)
    else library.push(entry)
  }
  const knownAssetIds = new Set(library.map((entry) => entry.assetId ?? entry.id))
  for (const project of savedProjects) {
    for (const media of project.document?.media ?? []) {
      const assetId = media.assetRef
      if (!assetId || knownAssetIds.has(assetId)) continue
      const expected = expectedFingerprints.get(assetId)
      const fingerprintConflict = Boolean(expected && expected.size > 1)
      const metadata = media.metadata
      const number = (value: unknown) => typeof value === 'number' && Number.isFinite(value) ? value : undefined
      const filename = typeof metadata.filename === 'string' && metadata.filename.trim()
        ? metadata.filename
        : `${assetId}.${media.kind === 'audio' ? 'wav' : 'mp4'}`
      library.push({
        id: assetId,
        assetId,
        fingerprint: fingerprintConflict ? undefined : media.contentFingerprint,
        identityConflict: fingerprintConflict,
        kind: 'source',
        filename,
        title: fingerprintConflict
          ? `${filename} — конфликт fingerprint проектов`
          : typeof metadata.title === 'string' ? metadata.title : filename,
        url: '',
        duration: number(metadata.duration),
        width: number(metadata.width),
        height: number(metadata.height),
        fps: number(metadata.fps),
        vcodec: typeof metadata.vcodec === 'string' ? metadata.vcodec : undefined,
        acodec: typeof metadata.acodec === 'string' ? metadata.acodec : undefined,
        mediaKind: media.kind === 'audio' ? 'audio' : 'video',
        sizeBytes: number(metadata.sizeBytes),
        availability: 'offline',
        createdAt: project.updatedAt,
      })
      knownAssetIds.add(assetId)
    }
  }
  library.sort((left, right) => right.createdAt - left.createdAt)
  return [...library]
}

async function resolveSourceRecord(sourceId: string, expectedOverride?: string): Promise<SourceRecord> {
  let cached = sources.get(sourceId)
  const expected = expectedOverride ? new Set([expectedOverride]) : new Set(
    (await allProjects()).flatMap((project) => project.document?.media ?? [])
      .filter((media) => (media.assetRef ?? media.id) === sourceId && media.contentFingerprint)
      .map((media) => media.contentFingerprint!),
  )
  const actualFingerprint = cached?.info.fingerprint
  if (expected.size > 1) {
    throw new BrowserAssetStorageError(
      'Fingerprint исходника не совпадает с сохранённым проектом. Выполните точный relink.',
      'fingerprint',
    )
  }
  if (expected.size === 1 && actualFingerprint && !expected.has(actualFingerprint)) {
    URL.revokeObjectURL(cached!.info.url)
    sources.delete(sourceId)
    cached = undefined
  }
  if (cached) return cached
  const asset = await getBrowserAsset(sourceId)
  if (expected.size === 1 && !expected.has(asset.fingerprint)) {
    throw new BrowserAssetStorageError(
      'Fingerprint исходника не совпадает с сохранённым проектом. Выполните точный relink.',
      'fingerprint',
    )
  }
  const file = new File([asset.file], asset.filename, { type: asset.fileType })
  const info: VideoInfo = {
    ...asset.info,
    assetId: asset.id,
    fingerprint: asset.fingerprint,
    url: objectUrl(file),
  }
  const record = { file, info }
  sources.set(sourceId, record)
  return record
}

/** Lazily materialize bytes and a fresh runtime URL for an opened source. */
export async function resolveSource(sourceId: string, expectedFingerprint?: string): Promise<VideoInfo> {
  return withProjectAssetLock(async () => (await resolveSourceRecord(sourceId, expectedFingerprint)).info)
}

export async function relinkSource(
  sourceId: string,
  file: File,
  handle?: FileSystemFileHandle,
  expectedMedia?: ProjectMedia,
): Promise<VideoInfo> {
  return withProjectAssetLock(async () => {
  // A retained File System Access handle stores only a locator; byte-for-byte
  // OPFS quota is needed solely for the fallback that persists the file itself.
  if (!handle) lastStorageEstimate = await prepareBrowserStorage(file.size * 2 + OPFS_INGEST_OVERHEAD_BYTES)
  const entry = library.find((candidate) => candidate.id === sourceId)
  const metadata = expectedMedia?.metadata
  const metadataNumber = (key: string) => typeof metadata?.[key] === 'number' ? metadata[key] as number : undefined
  const expected = expectedMedia?.contentFingerprint ?? entry?.fingerprint
  const recovery = expected ? {
    id: expectedMedia?.assetRef ?? entry?.assetId ?? sourceId,
    filename: typeof metadata?.filename === 'string' ? metadata.filename : entry?.filename ?? file.name,
    fileType: file.type,
    fingerprint: expected,
    byteLength: metadataNumber('sizeBytes') ?? entry?.sizeBytes ?? undefined,
    info: {
      id: sourceId,
      filename: typeof metadata?.filename === 'string' ? metadata.filename : entry?.filename ?? file.name,
      duration: metadataNumber('duration') ?? entry?.duration ?? 0,
      width: metadataNumber('width') ?? entry?.width ?? 0,
      height: metadataNumber('height') ?? entry?.height ?? 0,
      title: typeof metadata?.title === 'string' ? metadata.title : entry?.title,
      fps: metadataNumber('fps') ?? entry?.fps,
      vcodec: typeof metadata?.vcodec === 'string' ? metadata.vcodec : entry?.vcodec,
      acodec: typeof metadata?.acodec === 'string' ? metadata.acodec : entry?.acodec,
      mediaKind: expectedMedia?.kind === 'audio' ? 'audio' as const : entry?.mediaKind,
      assetId: expectedMedia?.assetRef ?? entry?.assetId ?? sourceId,
      fingerprint: expected,
      sizeBytes: metadataNumber('sizeBytes') ?? entry?.sizeBytes,
    },
    createdAt: entry?.createdAt,
  } : undefined
  await relinkBrowserAsset(sourceId, file, handle, recovery)
  const cached = sources.get(sourceId)
  if (cached) URL.revokeObjectURL(cached.info.url)
  sources.delete(sourceId)
  return (await resolveSourceRecord(sourceId, expected)).info
  })
}

export async function restoreExternalSource(sourceId: string, expectedFingerprint?: string): Promise<VideoInfo> {
  return withProjectAssetLock(async () => {
    await requestExternalHandleAccess(sourceId)
    const cached = sources.get(sourceId)
    if (cached) URL.revokeObjectURL(cached.info.url)
    sources.delete(sourceId)
    return (await resolveSourceRecord(sourceId, expectedFingerprint)).info
  })
}

export async function deleteLibraryItem(itemId: string): Promise<void> {
  const memoryOutputIndex = library.findIndex((entry) => entry.id === itemId && entry.kind === 'output')
  if (memoryOutputIndex >= 0) {
    URL.revokeObjectURL(library[memoryOutputIndex]!.url)
    library.splice(memoryOutputIndex, 1)
    return
  }
  if (!navigator.locks) {
    throw new Error(
      'Безопасное удаление недоступно в этом браузере: Web Locks API не поддерживается. Файл сохранён.',
    )
  }
  return withProjectAssetLock(async () => {
  const referencingProject = (await allProjects()).find((project) =>
    project.videoId === itemId
      || project.document?.media.some((media) => (media.assetRef ?? media.id) === itemId),
  )
  if (referencingProject) {
    throw new Error(`Файл используется в проекте «${referencingProject.name}»`)
  }
  const index = library.findIndex((entry) => entry.id === itemId)
  await deleteBrowserAsset(itemId)
  if (index >= 0) {
    URL.revokeObjectURL(library[index]!.url)
    library.splice(index, 1)
  }
  sources.delete(itemId)
  })
}

export async function saveProject(body: Record<string, unknown>): Promise<ProjectDto> {
  return withProjectAssetLock(async () => {
  const videoId = String(body.videoId)
  const now = Date.now()
  const expectedRevision = typeof body.expectedRevision === 'number' ? body.expectedRevision : undefined
  const requestedProjectId = typeof body.projectId === 'string' && body.projectId ? body.projectId : undefined
  const document = body.document
    ? migrateProjectDocument(body.document)
    : createProjectDocumentFromLegacy(
        videoId,
        String(body.name || 'Проект'),
        body.video as Record<string, unknown>,
        body.edit as Record<string, unknown>,
      )
  const persistedBeforeSave = requestedProjectId
    ? await projectById(requestedProjectId)
    : await projectByVideo(videoId)
  const identitiesByAssetRef = (items: ProjectMedia[]) => {
    const identities = new Map<string, Set<string>>()
    for (const media of items) {
      const assetRef = media.assetRef ?? media.id
      const values = identities.get(assetRef) ?? new Set<string>()
      values.add(media.contentFingerprint ?? '')
      identities.set(assetRef, values)
    }
    return identities
  }
  const previousIdentities = identitiesByAssetRef(persistedBeforeSave?.document?.media ?? [])
  const nextIdentities = identitiesByAssetRef(document.media)
  for (const [assetRef, nextFingerprints] of nextIdentities) {
    if (nextFingerprints.size > 1) {
      throw new Error(`Исходник «${assetRef}» имеет конфликтующие fingerprints`)
    }
    const previousFingerprints = previousIdentities.get(assetRef)
    if (previousFingerprints && (
      previousFingerprints.size !== nextFingerprints.size
      || [...previousFingerprints].some((value) => !nextFingerprints.has(value))
    )) {
      throw new Error(`Идентичность исходника «${assetRef}» нельзя изменить обычным сохранением; используйте relink`)
    }
  }
  const previousMediaIdsBeforeSave = new Set(
    persistedBeforeSave?.document?.media.map((media) => media.assetRef ?? media.id) ?? [],
  )
  const readyAssets = new Map((await auditBrowserAssets())
    .filter((asset) => asset.availability === 'ready')
    .map((asset) => [asset.id, asset]))
  const sessionAssets = new Map([...sources.entries()]
    .filter(([, source]) => source.info.availability === 'session' && source.info.fingerprint)
    .map(([assetId, source]) => [assetId, source]))
  for (const media of document.media) {
    const assetRef = media.assetRef ?? media.id
    const ready = readyAssets.get(assetRef)
    const session = sessionAssets.get(assetRef)
    if (session?.info.fingerprint && !media.contentFingerprint) {
      media.contentFingerprint = session.info.fingerprint
    }
    if (ready && media.contentFingerprint && ready.fingerprint !== media.contentFingerprint) {
      throw new Error(`Исходный файл «${assetRef}» конфликтует с fingerprint проекта`)
    }
    if (session && media.contentFingerprint && session.info.fingerprint !== media.contentFingerprint) {
      throw new Error(`Исходный файл «${assetRef}» конфликтует с fingerprint проекта`)
    }
    if (!previousMediaIdsBeforeSave.has(assetRef) && ready) {
      await getBrowserAsset(assetRef)
    }
  }
  const project = await compareAndSwapProject(
    videoId,
    requestedProjectId,
    expectedRevision,
    (previous) => {
      const previousMediaIds = new Set(previous?.document?.media.map((media) => media.assetRef ?? media.id) ?? [])
      const missingNewAsset = document.media.find((media) =>
        !previousMediaIds.has(media.assetRef ?? media.id)
        && !readyAssets.has(media.assetRef ?? media.id)
        && !sessionAssets.has(media.assetRef ?? media.id),
      )
      if (missingNewAsset) {
        throw new Error(`Исходный файл «${missingNewAsset.assetRef ?? missingNewAsset.id}» больше недоступен — найдите его повторно`)
      }
      return {
      id: requestedProjectId ?? previous?.id ?? id(),
      name: String(body.name || 'Проект'),
      videoId,
      video: body.video as VideoInfo,
      edit: body.edit as ProjectDto['edit'],
      document,
      revision: (previous?.revision ?? 0) + 1,
      createdAt: previous?.createdAt ?? now,
      updatedAt: now,
      }
    },
    () => new ProjectRevisionConflictError(),
    Number(body.writerWatermark) || 0,
  )
  projects.set(project.id, project)
  return project
  })
}

export async function getProjectByVideo(videoId: string): Promise<ProjectDto | null> {
  const project = [...projects.values()].find((item) => item.videoId === videoId) ?? await projectByVideo(videoId)
  if (project) projects.set(project.id, project)
  return project
}

export async function getProjects(): Promise<ProjectDto[]> {
  const persisted = await allProjects()
  for (const project of persisted) projects.set(project.id, project)
  return [...projects.values()].sort((a, b) => b.updatedAt - a.updatedAt)
}

export async function deleteProject(projectId: string): Promise<void> {
  projects.delete(projectId)
  await removeProject(projectId)
}
