import type {
  Capabilities,
  Job,
  LutAsset,
  MediaEntry,
  ResultInfo,
  VideoInfo,
} from './types'
import type { ProjectDto } from './api'

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

const sources = new Map<string, SourceRecord>()
const luts = new Map<string, LutRecord>()
const jobs = new Map<string, BrowserJob>()
const library: MediaEntry[] = []
const projects = new Map<string, ProjectDto>()

let ffmpegInstance: import('@ffmpeg/ffmpeg').FFmpeg | null = null
let ffmpegLoading: Promise<import('@ffmpeg/ffmpeg').FFmpeg> | null = null
let activeJobId: string | null = null

export class LinkImportRequiresServerError extends Error {
  constructor() {
    super('Импорт по ссылке будет доступен в полноценной версии сайта. В статической версии выберите файл с устройства.')
    this.name = 'LinkImportRequiresServerError'
  }
}

export function isBrowserProcessing(): boolean {
  return import.meta.env.VITE_PROCESSING_MODE === 'browser'
}

function id(): string {
  return crypto.randomUUID()
}

function objectUrl(blob: Blob): string {
  return URL.createObjectURL(blob)
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

export async function uploadFile(file: File): Promise<VideoInfo> {
  if (!file.type.startsWith('video/')) throw new Error('Выберите видеофайл')
  const metadata = await probeVideo(file)
  const sourceId = id()
  const info: VideoInfo = {
    id: sourceId,
    url: objectUrl(file),
    filename: file.name,
    ...metadata,
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
    sizeBytes: file.size,
    createdAt: Date.now(),
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
    await ffmpeg.load({
      coreURL: coreUrl('ffmpeg-core.js'),
      wasmURL: coreUrl('ffmpeg-core.wasm'),
    })
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
      return { filename: 'edited.webm', mime: 'video/webm', args: ['-c:v', 'libvpx-vp9', '-crf', String(quality), '-b:v', '0', '-c:a', 'libopus'] }
    case 'gif':
      return { filename: 'edited.gif', mime: 'image/gif', args: ['-an', '-loop', '0'] }
    case 'png':
      return { filename: 'frame.png', mime: 'image/png', args: ['-frames:v', '1', '-an'] }
    case 'jpg':
      return { filename: 'frame.jpg', mime: 'image/jpeg', args: ['-frames:v', '1', '-q:v', '2', '-an'] }
    case 'mp3':
      return { filename: 'audio.mp3', mime: 'audio/mpeg', args: ['-vn', '-c:a', 'libmp3lame', '-q:a', '2'] }
    default:
      return { filename: 'edited.mp4', mime: 'video/mp4', args: ['-c:v', 'libx264', '-preset', 'veryfast', '-crf', String(quality), '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-movflags', '+faststart'] }
  }
}

async function buildArgs(ffmpeg: import('@ffmpeg/ffmpeg').FFmpeg, source: SourceRecord, payload: EditPayload) {
  const extension = source.file.name.split('.').pop()?.replace(/[^a-z0-9]/gi, '') || 'mp4'
  const inputName = `input-${id()}.${extension}`
  await ffmpeg.writeFile(inputName, new Uint8Array(await source.file.arrayBuffer()))

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
  args.push(...output.args, output.filename)
  return { ffmpegArgs: args, inputName, filename: output.filename, mime: output.mime }
}

async function runJob(jobId: string, payload: EditPayload): Promise<void> {
  const job = jobs.get(jobId)
  if (!job) return
  const source = sources.get(String(payload.videoId))
  if (!source) {
    Object.assign(job, { status: 'error', error: 'Исходный файл больше недоступен — выберите его повторно' })
    return
  }
  try {
    job.status = 'running'
    job.stage = 'Загружаю FFmpeg в браузер…'
    job.progress = 1
    const ffmpeg = await loadFfmpeg()
    if (job.cancelled) throw new Error('cancelled')
    activeJobId = jobId
    const onProgress = ({ progress }: { progress: number }) => {
      job.progress = Math.max(2, Math.min(99, Math.round(progress * 100)))
      job.stage = 'Обрабатываю на этом устройстве…'
    }
    ffmpeg.on('progress', onProgress)
    const spec = await buildArgs(ffmpeg, source, payload)
    const exitCode = await ffmpeg.exec(spec.ffmpegArgs)
    ffmpeg.off('progress', onProgress)
    if (job.cancelled) throw new Error('cancelled')
    if (exitCode !== 0) throw new Error(`FFmpeg завершился с кодом ${exitCode}`)
    const data = await ffmpeg.readFile(spec.filename)
    const bytes = data instanceof Uint8Array ? data : new TextEncoder().encode(data)
    const copy = new Uint8Array(bytes.byteLength)
    copy.set(bytes)
    const blob = new Blob([copy.buffer], { type: spec.mime })
    const resultId = id()
    const result: ResultInfo = {
      id: resultId,
      url: objectUrl(blob),
      filename: spec.filename,
      sizeBytes: blob.size,
    }
    library.unshift({ id: resultId, kind: 'output', filename: spec.filename, url: result.url, sizeBytes: blob.size, createdAt: Date.now() })
    Object.assign(job, { status: 'done', progress: 100, stage: 'Готово', result })
    await ffmpeg.deleteFile(spec.inputName).catch(() => undefined)
    await ffmpeg.deleteFile(spec.filename).catch(() => undefined)
  } catch (error) {
    const cancelled = job.cancelled || (error instanceof Error && error.message === 'cancelled')
    Object.assign(job, cancelled
      ? { status: 'cancelled', error: undefined }
      : { status: 'error', error: error instanceof Error ? error.message : String(error) })
  } finally {
    if (activeJobId === jobId) activeJobId = null
  }
}

export function edit(payload: EditPayload): { jobId: string } {
  if (activeJobId) throw new Error('Дождитесь завершения текущего экспорта')
  const jobId = id()
  jobs.set(jobId, { id: jobId, status: 'pending', progress: 0, stage: 'Подготовка…' })
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

export function getCapabilities(): Capabilities {
  const enabled = (id: string, label = id) => ({ id, label, available: true })
  const disabled = (id: string, label: string, reason: string) => ({ id, label, available: false, reason })
  return {
    schemaVersion: 1,
    toolFingerprint: 'ffmpeg.wasm/client',
    formats: [
      enabled('mp4', 'MP4'), enabled('webm', 'WebM'), enabled('gif', 'GIF'),
      enabled('png', 'PNG'), enabled('jpg', 'JPG'), enabled('mp3', 'MP3'),
      disabled('av1', 'AV1', 'Недоступно в браузерной сборке'),
      disabled('prores', 'ProRes', 'Недоступно в браузерной сборке'),
    ],
    codecs: [
      enabled('h264', 'H.264'),
      disabled('h265', 'H.265', 'Недоступно в браузерной сборке'),
    ],
    filters: [
      ...['grayscale', 'sepia', 'warm', 'cold', 'teal-orange', 'faded', 'noir', 'vintage', 'lut3d', 'curves'].map((name) => enabled(name)),
      disabled('lut3d-blend', 'Интенсивность LUT', 'В браузере LUT применяется с интенсивностью 100%'),
    ],
    hardware: [disabled('native', 'Аппаратное ускорение', 'Используется WebAssembly')],
  }
}

export function getLibrary(): MediaEntry[] {
  return [...library]
}

export function deleteLibraryItem(itemId: string): void {
  const index = library.findIndex((entry) => entry.id === itemId)
  if (index >= 0) {
    URL.revokeObjectURL(library[index]!.url)
    library.splice(index, 1)
  }
  sources.delete(itemId)
}

export function saveProject(body: Record<string, unknown>): ProjectDto {
  const videoId = String(body.videoId)
  const now = Date.now()
  const previous = projects.get(videoId)
  const project: ProjectDto = {
    id: previous?.id ?? id(),
    name: String(body.name || 'Проект'),
    videoId,
    video: body.video as VideoInfo,
    edit: body.edit as ProjectDto['edit'],
    createdAt: previous?.createdAt ?? now,
    updatedAt: now,
  }
  projects.set(videoId, project)
  return project
}

export function getProjectByVideo(videoId: string): ProjectDto | null {
  return projects.get(videoId) ?? null
}

export function getProjects(): ProjectDto[] {
  return [...projects.values()].sort((a, b) => b.updatedAt - a.updatedAt)
}

export function deleteProject(projectId: string): void {
  for (const [videoId, project] of projects) {
    if (project.id === projectId) projects.delete(videoId)
  }
}
