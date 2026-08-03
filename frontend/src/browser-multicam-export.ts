import { MEMFS_FALLBACK_MAX_INPUT_BYTES, MEMFS_MAX_OUTPUT_BYTES } from './browser-resource-plan'

const ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/
const SHA256 = /^[a-f0-9]{64}$/
const MAX_SAFE = Number.MAX_SAFE_INTEGER
const MAX_ANGLES = 9
const MAX_INTERVALS = 10_000

export interface BrowserMulticamFlatten {
  contract: 'multicam-flatten-v1'
  timeBase: number
  durationTicks: number
  timelineStartTick: number
  sourceStartTick: number
  audioAngleId: string
  target: { width: number; height: number; fps: number }
  angles: Array<{
    id: string; mediaId: string; assetRef: string; fingerprint: string
    sourceOriginTick: number; rate: { numerator: number; denominator: number }
  }>
  intervals: Array<{
    decisionId: string; angleId: string; mediaId: string; outputStartTick: number; durationTicks: number
    sourceStart: { numerator: string; denominator: number }
    sourceEnd: { numerator: string; denominator: number }
  }>
}

export interface MaterializedMulticamInput {
  path: string
  sizeBytes: number
  hasAudio: boolean
}

export interface BrowserMulticamResourceEstimate {
  uniqueInputBytes: number
  estimatedOutputBytes: number
  estimatedPeakMemoryBytes: number
  risk: 'safe' | 'warning' | 'blocked'
  reason: string | null
}

export interface BrowserMulticamArgvOptions {
  inputs: ReadonlyMap<string, MaterializedMulticamInput>
  workerFs: boolean
  memoryBudgetBytes: number
  maxOutputBytes?: number
  postVideoFilters?: readonly string[]
  lutFilter?: string | null
  lutIntensity?: number
  postVideoAfterLutFilters?: readonly string[]
  postAudioFilters?: readonly string[]
  output: { filename: string; args: readonly string[] }
  muteAudio?: boolean
}

export interface BrowserMulticamArgvPlan {
  argv: string[]
  inputAssetRefs: string[]
  expectedDurationSeconds: number
  resources: BrowserMulticamResourceEstimate
}

export interface BrowserMulticamTiming {
  trimStartSeconds: number
  trimEndSeconds: number
  cutStartSeconds: number | null
  cutEndSeconds: number | null
  selectedSeconds: number
  outputSeconds: number
}

function finite(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

/** Normalize legacy edit timing against the clip-local flattened clock. Cut
 * coordinates are returned after trim rebases that clock to zero. */
export function browserMulticamTiming(
  value: unknown,
  edit: Record<string, unknown>,
): BrowserMulticamTiming {
  const contract = parseBrowserMulticamFlatten(value)
  const duration = (contract.timelineStartTick + contract.durationTicks) / contract.timeBase
  const trim = edit.trim && typeof edit.trim === 'object' && !Array.isArray(edit.trim)
    ? edit.trim as Record<string, unknown> : null
  const trimStartSeconds = Math.max(0, Math.min(duration, finite(trim?.start, 0)))
  const trimEndSeconds = Math.max(trimStartSeconds, Math.min(duration, finite(trim?.end, duration)))
  const segments = Array.isArray(edit.segments)
    ? edit.segments.filter(item => item && typeof item === 'object' && !Array.isArray(item)) as Record<string, unknown>[]
    : []
  let cutStartSeconds: number | null = null
  let cutEndSeconds: number | null = null
  let removedSeconds = 0
  if (segments.length === 2) {
    const absoluteCutStart = finite(segments[0]?.end, 0)
    const absoluteCutEnd = finite(segments[1]?.start, 0)
    const overlapStart = Math.max(trimStartSeconds, Math.min(trimEndSeconds, absoluteCutStart))
    const overlapEnd = Math.max(overlapStart, Math.min(trimEndSeconds, absoluteCutEnd))
    if (overlapEnd > overlapStart) {
      cutStartSeconds = overlapStart - trimStartSeconds
      cutEndSeconds = overlapEnd - trimStartSeconds
      removedSeconds = overlapEnd - overlapStart
    }
  }
  const selectedSeconds = Math.max(0, trimEndSeconds - trimStartSeconds - removedSeconds)
  const speed = Math.max(0.5, Math.min(2, finite(edit.speed, 1)))
  return { trimStartSeconds, trimEndSeconds, cutStartSeconds, cutEndSeconds, selectedSeconds, outputSeconds: selectedSeconds / speed }
}

function object(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`invalid ${label}`)
  return value as Record<string, unknown>
}

function exact(value: Record<string, unknown>, keys: readonly string[], label: string): void {
  const allowed = new Set(keys)
  if (Object.keys(value).some(key => !allowed.has(key))) throw new Error(`invalid ${label} keys`)
}

function id(value: unknown, label: string): string {
  if (typeof value !== 'string' || !ID.test(value)) throw new Error(`invalid ${label}`)
  return value
}

function integer(value: unknown, label: string, positive = false): number {
  if (!Number.isSafeInteger(value) || (value as number) < (positive ? 1 : 0) || (value as number) > MAX_SAFE) throw new Error(`invalid ${label}`)
  return value as number
}

function finitePositive(value: unknown, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) throw new Error(`invalid ${label}`)
  return value
}

function gcd(a: number, b: number): number {
  while (b) [a, b] = [b, a % b]
  return a
}

function rational(value: unknown, label: string): { numerator: string; denominator: number } {
  const raw = object(value, label); exact(raw, ['numerator', 'denominator'], label)
  if (typeof raw.numerator !== 'string' || !/^(0|[1-9]\d*)$/.test(raw.numerator)) throw new Error(`invalid ${label}.numerator`)
  const numerator = BigInt(raw.numerator)
  if (numerator > BigInt(MAX_SAFE)) throw new Error(`invalid ${label}.numerator`)
  return { numerator: raw.numerator, denominator: integer(raw.denominator, `${label}.denominator`, true) }
}

/** Parse an untrusted project/export extension. Unknown fields are rejected so
 * the renderer never silently assigns semantics to a newer contract. */
export function parseBrowserMulticamFlatten(value: unknown): BrowserMulticamFlatten {
  const root = object(value, 'multicam flatten')
  exact(root, ['contract', 'timeBase', 'durationTicks', 'timelineStartTick', 'sourceStartTick', 'audioAngleId', 'target', 'angles', 'intervals'], 'multicam flatten')
  if (root.contract !== 'multicam-flatten-v1') throw new Error('invalid multicam flatten contract')
  const timeBase = integer(root.timeBase, 'multicam timeBase', true)
  const durationTicks = integer(root.durationTicks, 'multicam durationTicks', true)
  const timelineStartTick = integer(root.timelineStartTick ?? 0, 'multicam timelineStartTick')
  if (timelineStartTick + durationTicks > MAX_SAFE) throw new Error('invalid multicam timeline duration')
  const sourceStartTick = integer(root.sourceStartTick ?? 0, 'multicam sourceStartTick')
  const targetRaw = object(root.target, 'multicam target'); exact(targetRaw, ['width', 'height', 'fps'], 'multicam target')
  const target = {
    width: integer(targetRaw.width, 'multicam target.width', true),
    height: integer(targetRaw.height, 'multicam target.height', true),
    fps: finitePositive(targetRaw.fps, 'multicam target.fps'),
  }
  if (target.width > 8192 || target.height > 8192 || target.fps > 240) throw new Error('invalid multicam target bounds')
  if (!Array.isArray(root.angles) || root.angles.length < 2 || root.angles.length > MAX_ANGLES) throw new Error('invalid multicam angles')
  const angleIds = new Set<string>(); const mediaIds = new Set<string>()
  const angles = root.angles.map((item, index) => {
    const raw = object(item, `multicam angle ${index}`)
    exact(raw, ['id', 'mediaId', 'assetRef', 'fingerprint', 'sourceOriginTick', 'rate'], `multicam angle ${index}`)
    const angleId = id(raw.id, 'multicam angle.id'); const mediaId = id(raw.mediaId, 'multicam angle.mediaId')
    if (angleIds.has(angleId) || mediaIds.has(mediaId)) throw new Error('duplicate multicam angle')
    angleIds.add(angleId); mediaIds.add(mediaId)
    if (typeof raw.fingerprint !== 'string' || !SHA256.test(raw.fingerprint)) throw new Error('invalid multicam fingerprint')
    const rateRaw = object(raw.rate, 'multicam rate'); exact(rateRaw, ['numerator', 'denominator'], 'multicam rate')
    const rate = { numerator: integer(rateRaw.numerator, 'multicam rate.numerator', true), denominator: integer(rateRaw.denominator, 'multicam rate.denominator', true) }
    if (gcd(rate.numerator, rate.denominator) !== 1) throw new Error('invalid multicam reduced rate')
    if (rate.numerator > rate.denominator * 16 || rate.denominator > rate.numerator * 16) throw new Error('invalid multicam rate bounds')
    return { id: angleId, mediaId, assetRef: id(raw.assetRef, 'multicam angle.assetRef'), fingerprint: raw.fingerprint, sourceOriginTick: integer(raw.sourceOriginTick, 'multicam sourceOriginTick'), rate }
  })
  const audioAngleId = id(root.audioAngleId, 'multicam audioAngleId')
  if (!angleIds.has(audioAngleId)) throw new Error('invalid multicam audioAngleId')
  if (!Array.isArray(root.intervals) || root.intervals.length < 1 || root.intervals.length > MAX_INTERVALS) throw new Error('invalid multicam intervals')
  let cursor = 0
  const decisions = new Set<string>()
  const angleById = new Map(angles.map(angle => [angle.id, angle]))
  const intervals = root.intervals.map((item, index) => {
    const raw = object(item, `multicam interval ${index}`)
    exact(raw, ['decisionId', 'angleId', 'mediaId', 'outputStartTick', 'durationTicks', 'sourceStart', 'sourceEnd'], `multicam interval ${index}`)
    const decisionId = id(raw.decisionId, 'multicam decisionId'); const angleId = id(raw.angleId, 'multicam interval.angleId')
    const mediaId = id(raw.mediaId, 'multicam interval.mediaId'); const outputStartTick = integer(raw.outputStartTick, 'multicam outputStartTick')
    const intervalDuration = integer(raw.durationTicks, 'multicam interval.durationTicks', true)
    if (decisions.has(decisionId) || outputStartTick !== cursor || angleById.get(angleId)?.mediaId !== mediaId || cursor + intervalDuration > durationTicks) throw new Error('invalid multicam interval coverage')
    decisions.add(decisionId); cursor += intervalDuration
    const sourceStart = rational(raw.sourceStart, 'multicam sourceStart'); const sourceEnd = rational(raw.sourceEnd, 'multicam sourceEnd')
    const angle = angleById.get(angleId)!
    const rateNumerator = BigInt(angle.rate.numerator); const rateDenominator = BigInt(angle.rate.denominator)
    const startDenominator = BigInt(sourceStart.denominator); const endDenominator = BigInt(sourceEnd.denominator)
    const startNumerator = BigInt(sourceStart.numerator); const endNumerator = BigInt(sourceEnd.numerator)
    const expectedStartNumerator = BigInt(angle.sourceOriginTick) * rateDenominator
      + BigInt(sourceStartTick + outputStartTick) * rateNumerator
    if (startNumerator * rateDenominator !== expectedStartNumerator * startDenominator) throw new Error('invalid multicam interval rational')
    const actualDelta = (endNumerator * startDenominator - startNumerator * endDenominator) * rateDenominator
    const expectedDelta = BigInt(intervalDuration) * rateNumerator * startDenominator * endDenominator
    if (actualDelta !== expectedDelta) throw new Error('invalid multicam interval rational')
    return { decisionId, angleId, mediaId, outputStartTick, durationTicks: intervalDuration, sourceStart, sourceEnd }
  })
  if (cursor !== durationTicks) throw new Error('invalid multicam interval coverage')
  return { contract: 'multicam-flatten-v1', timeBase, durationTicks, timelineStartTick, sourceStartTick, audioAngleId, target, angles, intervals }
}

export function estimateBrowserMulticamResources(
  contract: BrowserMulticamFlatten,
  inputs: ReadonlyMap<string, MaterializedMulticamInput>,
  options: { workerFs: boolean; memoryBudgetBytes: number; maxOutputBytes?: number },
): BrowserMulticamResourceEstimate {
  const uniqueRefs = [...new Set(contract.angles.map(angle => angle.assetRef))]
  let uniqueInputBytes = 0
  for (const ref of uniqueRefs) {
    const input = inputs.get(ref)
    if (!input || !Number.isSafeInteger(input.sizeBytes) || input.sizeBytes < 0) throw new Error(`missing multicam input ${ref}`)
    uniqueInputBytes += input.sizeBytes
    if (!Number.isSafeInteger(uniqueInputBytes)) throw new Error('invalid multicam aggregate input size')
  }
  const seconds = (contract.timelineStartTick + contract.durationTicks) / contract.timeBase
  const rawRate = contract.target.width * contract.target.height * contract.target.fps * 1.5
  const maxOutputBytes = options.maxOutputBytes ?? MEMFS_MAX_OUTPUT_BYTES
  const estimatedOutputBytes = Math.min(maxOutputBytes, Math.ceil(seconds * Math.min(rawRate, 8_000_000 / 8)))
  const estimatedPeakMemoryBytes = estimatedOutputBytes + 64 * 1024 * 1024 + (options.workerFs ? 0 : uniqueInputBytes)
  const memfsBlocked = !options.workerFs && uniqueInputBytes > MEMFS_FALLBACK_MAX_INPUT_BYTES
  const memoryBlocked = estimatedPeakMemoryBytes > options.memoryBudgetBytes
  return {
    uniqueInputBytes, estimatedOutputBytes, estimatedPeakMemoryBytes,
    risk: memfsBlocked || memoryBlocked ? 'blocked' : estimatedPeakMemoryBytes > options.memoryBudgetBytes * 0.8 ? 'warning' : 'safe',
    reason: memfsBlocked ? 'Multicam inputs exceed bounded MEMFS fallback; WORKERFS is required.' : memoryBlocked ? 'Multicam export exceeds the browser memory budget.' : null,
  }
}

function decimal(numerator: string, denominator: number): string {
  const value = Number(BigInt(numerator)) / denominator
  if (!Number.isFinite(value)) throw new Error('invalid multicam timestamp')
  return value.toFixed(9).replace(/\.?0+$/, '') || '0'
}

function rationalDeltaSeconds(
  start: { numerator: string; denominator: number },
  end: { numerator: string; denominator: number },
  timeBase: number,
): string {
  const numerator = BigInt(end.numerator) * BigInt(start.denominator)
    - BigInt(start.numerator) * BigInt(end.denominator)
  const denominator = BigInt(start.denominator) * BigInt(end.denominator) * BigInt(timeBase)
  if (numerator <= 0n) throw new Error('invalid multicam source duration')
  const value = Number(numerator) / Number(denominator)
  if (!Number.isFinite(value) || value <= 0) throw new Error('invalid multicam source duration')
  return value.toFixed(9).replace(/\.?0+$/, '')
}

function safePath(value: string, label: string): string {
  if (!value || /[\0\r\n]/.test(value)) throw new Error(`invalid ${label}`)
  return value
}

function trustedFilters(filters: readonly string[] | undefined): string[] {
  return (filters ?? []).map(filter => {
    if (!filter || [';', '[', ']', '\r', '\n'].some(character => filter.includes(character))) throw new Error('invalid injected FFmpeg filter')
    return filter
  })
}

/** Build argv only; files must already be materialized at the supplied virtual
 * paths. The caller retains ownership of mounts, progress, execution and cleanup. */
export function buildBrowserMulticamFfmpegArgv(value: unknown, options: BrowserMulticamArgvOptions): BrowserMulticamArgvPlan {
  const contract = parseBrowserMulticamFlatten(value)
  const resources = estimateBrowserMulticamResources(contract, options.inputs, options)
  if (resources.risk === 'blocked') throw new Error(resources.reason ?? 'Multicam export is blocked')
  const refs = [...new Set(contract.angles.map(angle => angle.assetRef))].sort()
  const inputIndex = new Map(refs.map((ref, index) => [ref, index]))
  const argv: string[] = []
  for (const ref of refs) argv.push('-i', safePath(options.inputs.get(ref)!.path, 'multicam input path'))
  const angleById = new Map(contract.angles.map(angle => [angle.id, angle]))
  const chains: string[] = []
  for (const [index, interval] of contract.intervals.entries()) {
    const angle = angleById.get(interval.angleId)!
    const sourceIndex = inputIndex.get(angle.assetRef)!
    const start = decimal(interval.sourceStart.numerator, interval.sourceStart.denominator * contract.timeBase)
    const duration = rationalDeltaSeconds(interval.sourceStart, interval.sourceEnd, contract.timeBase)
    const speed = angle.rate.numerator / angle.rate.denominator
    chains.push(`[${sourceIndex}:v]trim=start=${start}:duration=${duration},setpts=(PTS-STARTPTS)/${speed.toFixed(9).replace(/\.?0+$/, '')},scale=${contract.target.width}:${contract.target.height}:force_original_aspect_ratio=decrease,pad=${contract.target.width}:${contract.target.height}:(ow-iw)/2:(oh-ih)/2,fps=${contract.target.fps},setsar=1[v${index}]`)
  }
  const gapSeconds = contract.timelineStartTick / contract.timeBase
  if (contract.timelineStartTick > 0) {
    chains.push(`color=c=black:s=${contract.target.width}x${contract.target.height}:r=${contract.target.fps}:d=${gapSeconds.toFixed(9).replace(/\.?0+$/, '')},setsar=1[vgap]`)
  }
  const videoSegments = `${contract.timelineStartTick > 0 ? '[vgap]' : ''}${contract.intervals.map((_, index) => `[v${index}]`).join('')}`
  chains.push(`${videoSegments}concat=n=${contract.intervals.length + (contract.timelineStartTick > 0 ? 1 : 0)}:v=1:a=0[vflat]`)
  const postVideo = trustedFilters(options.postVideoFilters)
  const afterLut = trustedFilters(options.postVideoAfterLutFilters)
  const lutFilter = options.lutFilter ? trustedFilters([options.lutFilter])[0]! : null
  const lutIntensity = options.lutIntensity ?? (lutFilter ? 1 : 0)
  if (!Number.isFinite(lutIntensity) || lutIntensity < 0 || lutIntensity > 1) throw new Error('invalid multicam LUT intensity')
  let videoLabel = 'vflat'
  if (lutFilter && lutIntensity > 0 && lutIntensity < 1) {
    const before = postVideo.length ? `${postVideo.join(',')},` : ''
    const after = afterLut.length ? `,${afterLut.join(',')}` : ''
    chains.push(`[vflat]${before}split=2[lutbase][lutin]`)
    chains.push(`[lutin]${lutFilter}[lutapplied]`)
    chains.push(`[lutbase][lutapplied]blend=all_expr='A*(1-${lutIntensity.toFixed(6)})+B*${lutIntensity.toFixed(6)}'${after}[vout]`)
    videoLabel = 'vout'
  } else {
    const linear = [...postVideo, ...(lutFilter && lutIntensity > 0 ? [lutFilter] : []), ...afterLut]
    if (linear.length) { chains.push(`[vflat]${linear.join(',')}[vout]`); videoLabel = 'vout' }
  }
  const audioAngle = angleById.get(contract.audioAngleId)!
  const audioInput = options.inputs.get(audioAngle.assetRef)!
  let audioLabel: string | null = null
  if (audioInput.hasAudio && !options.muteAudio) {
    const audioStartNumerator = BigInt(audioAngle.sourceOriginTick) * BigInt(audioAngle.rate.denominator)
      + BigInt(contract.sourceStartTick) * BigInt(audioAngle.rate.numerator)
    const audioStart = decimal(audioStartNumerator.toString(), audioAngle.rate.denominator * contract.timeBase)
    const sourceDurationNumerator = BigInt(contract.durationTicks) * BigInt(audioAngle.rate.numerator)
    const sourceDurationDenominator = BigInt(contract.timeBase) * BigInt(audioAngle.rate.denominator)
    const sourceDurationValue = Number(sourceDurationNumerator) / Number(sourceDurationDenominator)
    if (!Number.isFinite(sourceDurationValue) || sourceDurationValue <= 0) throw new Error('invalid multicam audio duration')
    const sourceDuration = sourceDurationValue.toFixed(9).replace(/\.?0+$/, '')
    const audioFilters = [`atrim=start=${audioStart}:duration=${sourceDuration}`, 'asetpts=PTS-STARTPTS']
    if (audioAngle.rate.numerator !== audioAngle.rate.denominator) {
      let tempo = audioAngle.rate.numerator / audioAngle.rate.denominator
      while (tempo > 2) { audioFilters.push('atempo=2'); tempo /= 2 }
      while (tempo < .5) { audioFilters.push('atempo=.5'); tempo /= .5 }
      if (Math.abs(tempo - 1) > 1e-9) audioFilters.push(`atempo=${tempo.toFixed(9).replace(/\.?0+$/, '')}`)
    }
    if (contract.timelineStartTick > 0) audioFilters.push(`adelay=${(gapSeconds * 1_000).toFixed(6).replace(/\.?0+$/, '')}:all=1`)
    audioFilters.push(...trustedFilters(options.postAudioFilters))
    chains.push(`[${inputIndex.get(audioAngle.assetRef)!}:a]${audioFilters.join(',')}[aout]`)
    audioLabel = 'aout'
  }
  const filename = safePath(options.output.filename, 'multicam output filename')
  if (filename.startsWith('-')) throw new Error('invalid multicam output filename')
  argv.push('-filter_complex', chains.join(';'), '-map', `[${videoLabel}]`)
  if (audioLabel) argv.push('-map', `[${audioLabel}]`)
  else argv.push('-an')
  argv.push('-fs', String(options.maxOutputBytes ?? MEMFS_MAX_OUTPUT_BYTES), ...options.output.args, filename)
  return { argv, inputAssetRefs: refs, expectedDurationSeconds: (contract.timelineStartTick + contract.durationTicks) / contract.timeBase, resources }
}
