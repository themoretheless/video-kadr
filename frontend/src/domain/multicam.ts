import type { ProjectMedia } from '../project-schema'

export const MULTICAM_CONTRACT = 'multicam-v1' as const
export const MULTICAM_MIN_ANGLES = 2
export const MULTICAM_MAX_ANGLES = 9
export const MULTICAM_MAX_DECISIONS = 10_000
export const MULTICAM_MAX_CORRELATION_SAMPLES = 262_144
export const MULTICAM_MAX_CORRELATION_WORK = 8_000_000

export interface MulticamRate { numerator: number; denominator: number }
export interface MulticamAngle {
  id: string
  mediaId: string
  label: string
  sourceOriginTick: number
  rate: MulticamRate
  enabled: boolean
}
export interface MulticamSync {
  method: 'audio' | 'timecode' | 'marker'
  algorithmVersion: string
  confidence?: number
}
export interface MulticamGroup {
  contract: typeof MULTICAM_CONTRACT
  id: string
  name: string
  timeBase: number
  durationTicks: number
  referenceAngleId: string
  audioAngleId: string
  sync: MulticamSync
  angles: MulticamAngle[]
}
export interface MulticamDecision { id: string; offsetTick: number; angleId: string }
export interface TimecodeAnchor {
  angleId: string
  startFrame: number
  rate: MulticamRate
  dropFrame: boolean
}
export interface MarkerAnchor { angleId: string; sourceTick: number }
export interface RationalPosition { numerator: string; denominator: number }
export interface FlattenedMulticamInterval {
  decisionId: string
  angleId: string
  mediaId: string
  outputStartTick: number
  durationTicks: number
  sourceStart: RationalPosition
  sourceEnd: RationalPosition
}

export class MulticamError extends Error {
  constructor(readonly code:
    | 'invalid_group' | 'invalid_media' | 'invalid_sync' | 'invalid_decision'
    | 'out_of_bounds' | 'ambiguous_audio' | 'audio_budget') {
    super(`Multicam failed: ${code}`)
    this.name = 'MulticamError'
  }
}

function fail(code: MulticamError['code']): never { throw new MulticamError(code) }
function id(value: unknown): value is string {
  return typeof value === 'string' && value.trim() === value && value.length > 0 && value.length <= 128
}
function safeTick(value: unknown, positive = false): value is number {
  return Number.isSafeInteger(value) && (positive ? Number(value) > 0 : Number(value) >= 0)
}
function rate(value: MulticamRate): MulticamRate {
  if (!Number.isSafeInteger(value?.numerator) || value.numerator <= 0
    || !Number.isSafeInteger(value?.denominator) || value.denominator <= 0) fail('invalid_group')
  const divisor = gcd(value.numerator, value.denominator)
  return { numerator: value.numerator / divisor, denominator: value.denominator / divisor }
}
function gcd(left: number, right: number): number {
  while (right) [left, right] = [right, left % right]
  return Math.abs(left)
}
function mediaDurationTicks(media: ProjectMedia, timeBase: number): number {
  const duration = media.metadata?.duration
  if (typeof duration !== 'number' || !Number.isFinite(duration) || duration <= 0) fail('invalid_media')
  const ticks = Math.round(duration * timeBase)
  if (!safeTick(ticks, true)) fail('invalid_media')
  return ticks
}

export function validateMulticamGroup(group: MulticamGroup, media: readonly ProjectMedia[]): void {
  if (!group || typeof group !== 'object' || !Array.isArray(group.angles) || !Array.isArray(media)
    || group.contract !== MULTICAM_CONTRACT || !id(group.id) || typeof group.name !== 'string'
    || !group.name.trim() || group.name.length > 256
    || !safeTick(group.timeBase, true) || !safeTick(group.durationTicks, true)
    || group.angles.length < MULTICAM_MIN_ANGLES || group.angles.length > MULTICAM_MAX_ANGLES
    || !['audio', 'timecode', 'marker'].includes(group.sync?.method)
    || !id(group.sync?.algorithmVersion)) fail('invalid_group')
  if (group.sync.confidence !== undefined
    && (!Number.isFinite(group.sync.confidence) || group.sync.confidence < 0 || group.sync.confidence > 1)) fail('invalid_group')
  const mediaById = new Map(media.map(item => [item.id, item]))
  const angleIds = new Set<string>()
  const mediaIds = new Set<string>()
  for (const angle of group.angles) {
    if (!id(angle.id) || angleIds.has(angle.id) || !id(angle.mediaId) || mediaIds.has(angle.mediaId)
      || typeof angle.label !== 'string' || !angle.label.trim() || angle.label.length > 256
      || !safeTick(angle.sourceOriginTick) || typeof angle.enabled !== 'boolean') fail('invalid_group')
    angleIds.add(angle.id); mediaIds.add(angle.mediaId)
    const source = mediaById.get(angle.mediaId)
    if (!source || source.kind !== 'video') fail('invalid_media')
    const normalizedRate = rate(angle.rate)
    if (normalizedRate.numerator !== angle.rate.numerator || normalizedRate.denominator !== angle.rate.denominator
      || angle.rate.numerator > angle.rate.denominator * 16 || angle.rate.denominator > angle.rate.numerator * 16) fail('invalid_group')
    const end = rationalSourcePosition(angle, group.durationTicks)
    const sourceTicks = BigInt(mediaDurationTicks(source, group.timeBase))
    if (BigInt(angle.sourceOriginTick) * BigInt(end.denominator) < 0n
      || BigInt(end.numerator) > sourceTicks * BigInt(end.denominator)) fail('out_of_bounds')
  }
  if (!angleIds.has(group.referenceAngleId) || !angleIds.has(group.audioAngleId)) fail('invalid_group')
}

export function createMulticamGroup(input: {
  id: string; name: string; timeBase: number; media: readonly ProjectMedia[]; referenceMediaId?: string
}): MulticamGroup {
  if (!safeTick(input.timeBase, true) || input.media.length < MULTICAM_MIN_ANGLES || input.media.length > MULTICAM_MAX_ANGLES) fail('invalid_group')
  const seen = new Set<string>()
  const angles = input.media.map((media, index): MulticamAngle => {
    if (!id(media.id) || seen.has(media.id) || media.kind !== 'video') fail('invalid_media')
    seen.add(media.id)
    return {
      id: `angle-${index + 1}`,
      mediaId: media.id,
      label: typeof media.metadata?.title === 'string' && media.metadata.title.trim()
        ? media.metadata.title.trim() : typeof media.metadata?.filename === 'string' ? media.metadata.filename : `Камера ${index + 1}`,
      sourceOriginTick: 0,
      rate: { numerator: 1, denominator: 1 },
      enabled: true,
    }
  })
  const reference = angles.find(angle => angle.mediaId === (input.referenceMediaId ?? input.media[0]!.id))
  if (!reference) fail('invalid_media')
  const group: MulticamGroup = {
    contract: MULTICAM_CONTRACT, id: input.id, name: input.name, timeBase: input.timeBase,
    durationTicks: Math.min(...input.media.map(media => mediaDurationTicks(media, input.timeBase))),
    referenceAngleId: reference.id, audioAngleId: reference.id,
    sync: { method: 'marker', algorithmVersion: 'manual-zero-v1' }, angles,
  }
  validateMulticamGroup(group, input.media)
  return group
}

function withOrigins(group: MulticamGroup, origins: ReadonlyMap<string, number>, sync: MulticamSync, media: readonly ProjectMedia[]): MulticamGroup {
  const mediaById = new Map(media.map(item => [item.id, item]))
  const angles = group.angles.map(angle => {
    const origin = origins.get(angle.id)
    if (!safeTick(origin)) fail('invalid_sync')
    return { ...angle, sourceOriginTick: origin }
  })
  const durationTicks = Math.min(...angles.map(angle => {
    const source = mediaById.get(angle.mediaId)
    if (!source) fail('invalid_media')
    const remaining = mediaDurationTicks(source, group.timeBase) - angle.sourceOriginTick
    return Math.floor(remaining * angle.rate.denominator / angle.rate.numerator)
  }))
  if (!safeTick(durationTicks, true)) fail('invalid_sync')
  const result = { ...group, durationTicks, sync, angles }
  validateMulticamGroup(result, media)
  return result
}

export function syncMulticamByMarkers(group: MulticamGroup, anchors: readonly MarkerAnchor[], media: readonly ProjectMedia[]): MulticamGroup {
  validateMulticamGroup(group, media)
  if (anchors.length !== group.angles.length) fail('invalid_sync')
  const values = new Map<string, number>()
  for (const anchor of anchors) {
    if (!group.angles.some(angle => angle.id === anchor.angleId) || values.has(anchor.angleId) || !safeTick(anchor.sourceTick)) fail('invalid_sync')
    values.set(anchor.angleId, anchor.sourceTick)
  }
  return withOrigins(group, values, { method: 'marker', algorithmVersion: 'marker-anchor-v1' }, media)
}

export function syncMulticamByTimecode(group: MulticamGroup, anchors: readonly TimecodeAnchor[], media: readonly ProjectMedia[]): MulticamGroup {
  validateMulticamGroup(group, media)
  if (anchors.length !== group.angles.length) fail('invalid_sync')
  const absolute = new Map<string, { numerator: bigint; denominator: bigint }>()
  for (const anchor of anchors) {
    if (!group.angles.some(angle => angle.id === anchor.angleId) || absolute.has(anchor.angleId)
      || !safeTick(anchor.startFrame) || typeof anchor.dropFrame !== 'boolean') fail('invalid_sync')
    const normalized = rate(anchor.rate)
    // startFrame is the already decoded SMPTE frame number; drop-frame affects parsing, not elapsed-frame math.
    absolute.set(anchor.angleId, {
      numerator: BigInt(anchor.startFrame) * BigInt(normalized.denominator),
      denominator: BigInt(normalized.numerator),
    })
  }
  const latest = [...absolute.values()].reduce((max, value) =>
    value.numerator * max.denominator > max.numerator * value.denominator ? value : max)
  const origins = new Map<string, number>()
  for (const angle of group.angles) {
    const value = absolute.get(angle.id)!
    const deltaNumerator = latest.numerator * value.denominator - value.numerator * latest.denominator
    const deltaDenominator = latest.denominator * value.denominator
    const ticks = Number((deltaNumerator * BigInt(group.timeBase) + deltaDenominator / 2n) / deltaDenominator)
    if (!safeTick(ticks)) fail('invalid_sync')
    origins.set(angle.id, ticks)
  }
  return withOrigins(group, origins, { method: 'timecode', algorithmVersion: 'smpte-frame-v1' }, media)
}

export interface AudioCorrelationResult { offsetSamples: number; peak: number; confidence: number }
export function correlateMulticamAudio(reference: Float32Array, candidate: Float32Array, maxLag: number): AudioCorrelationResult {
  if (reference.length < 128 || candidate.length < 128
    || reference.length > MULTICAM_MAX_CORRELATION_SAMPLES || candidate.length > MULTICAM_MAX_CORRELATION_SAMPLES
    || !Number.isSafeInteger(maxLag) || maxLag < 0 || maxLag > Math.min(reference.length, candidate.length) - 64) fail('audio_budget')
  const lagCount = maxLag * 2 + 1
  if (Math.min(reference.length, candidate.length) * lagCount > MULTICAM_MAX_CORRELATION_WORK) fail('audio_budget')
  if (![...reference, ...candidate].every(Number.isFinite)) fail('invalid_sync')
  const mean = (values: Float32Array) => values.reduce((sum, value) => sum + value, 0) / values.length
  const referenceMean = mean(reference); const candidateMean = mean(candidate)
  const scores: Array<{ lag: number; score: number }> = []
  for (let lag = -maxLag; lag <= maxLag; lag++) {
    const rStart = Math.max(0, -lag); const cStart = Math.max(0, lag)
    const count = Math.min(reference.length - rStart, candidate.length - cStart)
    let cross = 0; let leftEnergy = 0; let rightEnergy = 0
    for (let index = 0; index < count; index++) {
      const left = reference[rStart + index]! - referenceMean
      const right = candidate[cStart + index]! - candidateMean
      cross += left * right; leftEnergy += left * left; rightEnergy += right * right
    }
    scores.push({ lag, score: leftEnergy > 1e-12 && rightEnergy > 1e-12 ? cross / Math.sqrt(leftEnergy * rightEnergy) : 0 })
  }
  scores.sort((left, right) => right.score - left.score || Math.abs(left.lag) - Math.abs(right.lag) || left.lag - right.lag)
  const best = scores[0]!
  const second = scores.find(item => Math.abs(item.lag - best.lag) > 2)?.score ?? -1
  const confidence = Math.max(0, Math.min(1, best.score - Math.max(0, second)))
  if (best.score < 0.35 || confidence < 0.05) fail('ambiguous_audio')
  return { offsetSamples: best.lag, peak: best.score, confidence }
}

export function validateMulticamDecisions(group: MulticamGroup, decisions: readonly MulticamDecision[], frameTicks: number): void {
  if (!Array.isArray(decisions) || !safeTick(frameTicks, true) || !safeTick(group?.durationTicks, true)
    || !Array.isArray(group?.angles) || decisions.length < 1 || decisions.length > MULTICAM_MAX_DECISIONS) fail('invalid_decision')
  const angles = new Set(group.angles.filter(angle => angle.enabled).map(angle => angle.id))
  const ids = new Set<string>(); let previousTick = -1; let previousAngle = ''
  for (const decision of decisions) {
    if (!id(decision.id) || ids.has(decision.id) || !angles.has(decision.angleId)
      || !safeTick(decision.offsetTick) || decision.offsetTick >= group.durationTicks
      || decision.offsetTick % frameTicks !== 0 || decision.offsetTick <= previousTick
      || decision.angleId === previousAngle) fail('invalid_decision')
    ids.add(decision.id); previousTick = decision.offsetTick; previousAngle = decision.angleId
  }
  if (decisions[0]!.offsetTick !== 0) fail('invalid_decision')
}

export function insertOrReplaceMulticamDecision(group: MulticamGroup, decisions: readonly MulticamDecision[], decision: MulticamDecision, frameTicks: number): MulticamDecision[] {
  if (!id(decision.id) || !safeTick(decision.offsetTick) || decision.offsetTick % frameTicks !== 0) fail('invalid_decision')
  const next = decisions.filter(item => item.offsetTick !== decision.offsetTick && item.id !== decision.id)
  next.push({ ...decision }); next.sort((a, b) => a.offsetTick - b.offsetTick || a.id.localeCompare(b.id))
  const coalesced = next.filter((item, index) => index === 0 || item.angleId !== next[index - 1]!.angleId)
  validateMulticamDecisions(group, coalesced, frameTicks)
  return coalesced
}

export function deleteMulticamDecision(group: MulticamGroup, decisions: readonly MulticamDecision[], decisionId: string, frameTicks: number): MulticamDecision[] {
  const existing = decisions.find(item => item.id === decisionId)
  if (!existing || existing.offsetTick === 0) fail('invalid_decision')
  const next = decisions.filter(item => item.id !== decisionId)
  const coalesced = next.filter((item, index) => index === 0 || item.angleId !== next[index - 1]!.angleId)
  validateMulticamDecisions(group, coalesced, frameTicks)
  return coalesced
}

export function activeMulticamAngle(group: MulticamGroup, decisions: readonly MulticamDecision[], offsetTick: number, frameTicks: number): MulticamAngle {
  validateMulticamDecisions(group, decisions, frameTicks)
  if (!safeTick(offsetTick) || offsetTick >= group.durationTicks) fail('out_of_bounds')
  let low = 0; let high = decisions.length
  while (low < high) { const middle = (low + high) >>> 1; if (decisions[middle]!.offsetTick <= offsetTick) low = middle + 1; else high = middle }
  const angle = group.angles.find(item => item.id === decisions[Math.max(0, low - 1)]!.angleId)
  return angle ?? fail('invalid_decision')
}

function rationalSourcePosition(angle: MulticamAngle, groupTick: number): RationalPosition {
  const denominator = angle.rate.denominator
  const numerator = BigInt(angle.sourceOriginTick) * BigInt(denominator) + BigInt(groupTick) * BigInt(angle.rate.numerator)
  return { numerator: numerator.toString(), denominator }
}

export function multicamAngleSourceSeconds(group: MulticamGroup, angleId: string, outputTick: number): number {
  if (!safeTick(outputTick) || outputTick >= group.durationTicks) fail('out_of_bounds')
  const angle = group.angles.find(item => item.id === angleId)
  if (!angle) fail('invalid_decision')
  const position = rationalSourcePosition(angle, outputTick)
  const seconds = Number(BigInt(position.numerator)) / position.denominator / group.timeBase
  if (!Number.isFinite(seconds) || seconds < 0) fail('out_of_bounds')
  return seconds
}

export function multicamOutputSecondsForAngle(group: MulticamGroup, angleId: string, sourceSeconds: number): number {
  const angle = group.angles.find(item => item.id === angleId)
  if (!angle || !Number.isFinite(sourceSeconds) || sourceSeconds < 0) fail('out_of_bounds')
  const sourceTick = sourceSeconds * group.timeBase
  const outputTick = (sourceTick - angle.sourceOriginTick) * angle.rate.denominator / angle.rate.numerator
  if (!Number.isFinite(outputTick) || outputTick < 0 || outputTick >= group.durationTicks + 1) fail('out_of_bounds')
  return outputTick / group.timeBase
}

export function compileFlattenedMulticamIntervals(group: MulticamGroup, decisions: readonly MulticamDecision[], frameTicks: number): FlattenedMulticamInterval[] {
  validateMulticamDecisions(group, decisions, frameTicks)
  return decisions.map((decision, index) => {
    const end = decisions[index + 1]?.offsetTick ?? group.durationTicks
    const angle = group.angles.find(item => item.id === decision.angleId) ?? fail('invalid_decision')
    return {
      decisionId: decision.id, angleId: angle.id, mediaId: angle.mediaId,
      outputStartTick: decision.offsetTick, durationTicks: end - decision.offsetTick,
      sourceStart: rationalSourcePosition(angle, decision.offsetTick),
      sourceEnd: rationalSourcePosition(angle, end),
    }
  })
}

export function multicamFingerprintInput(group: MulticamGroup, decisions: readonly MulticamDecision[], frameTicks: number, media: readonly ProjectMedia[]): string {
  validateMulticamGroup(group, media)
  const intervals = compileFlattenedMulticamIntervals(group, decisions, frameTicks)
  const fingerprints = new Map(media.map(item => [item.id, item.contentFingerprint]))
  const sources = group.angles.map(angle => {
    const fingerprint = fingerprints.get(angle.mediaId)
    if (typeof fingerprint !== 'string' || !/^[a-f0-9]{64}$/.test(fingerprint)) fail('invalid_media')
    return { angleId: angle.id, mediaId: angle.mediaId, fingerprint }
  })
  const canonicalGroup = {
    contract: group.contract,
    id: group.id,
    name: group.name,
    timeBase: group.timeBase,
    durationTicks: group.durationTicks,
    referenceAngleId: group.referenceAngleId,
    audioAngleId: group.audioAngleId,
    sync: {
      method: group.sync.method,
      algorithmVersion: group.sync.algorithmVersion,
      ...(group.sync.confidence === undefined ? {} : { confidence: group.sync.confidence }),
    },
    angles: group.angles.map(angle => ({
      id: angle.id, mediaId: angle.mediaId, label: angle.label,
      sourceOriginTick: angle.sourceOriginTick,
      rate: { numerator: angle.rate.numerator, denominator: angle.rate.denominator },
      enabled: angle.enabled,
    })),
  }
  const canonicalDecisions = decisions.map(decision => ({
    id: decision.id, offsetTick: decision.offsetTick, angleId: decision.angleId,
  }))
  return JSON.stringify({
    contract: 'multicam-flatten-v1', group: canonicalGroup,
    decisions: canonicalDecisions, intervals, sources, frameTicks,
  })
}
