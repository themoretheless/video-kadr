// Extra audio tracks (music, voiceover, sfx) plus the dynamics chain applied to
// the mixed result. Foundation ships state, serialization and validated
// restore; the audio agent adds the mixing UI on top.

import { reactive } from 'vue'
import type {
  AudioDynamics,
  AudioRole,
  AudioTrackSpec,
  CompressorSpec,
  DuckingSpec,
  GateSpec,
  Interpolation,
  Keyframe,
  KeyframeTrack,
  LimiterSpec,
} from '../types'
import {
  assetIdOr,
  boolOr,
  clampInt,
  clampNullable,
  clampNumber,
  cloneTrack,
  enumOr,
  isRecord,
  MAX_AUDIO_TRACKS,
  MAX_KEYFRAMES,
  sanitizeKeyframeTrack,
  sanitizeList,
} from './validation'

const AUDIO_ROLES: readonly AudioRole[] = ['music', 'voiceover', 'sfx']
const MAX_TIMELINE_SECONDS = 24 * 60 * 60
const MAX_FADE_SECONDS = 60
const DEFAULT_BITRATE_KBPS = 128

/** Every dynamics field is materialised so the UI can bind to it directly. */
export interface AudioDynamicsState {
  denoise: number
  dereverb: boolean
  compressor: CompressorSpec | null
  limiter: LimiterSpec | null
  gate: GateSpec | null
  deesser: boolean
  highpassHz: number | null
  lowpassHz: number | null
  bitrateKbps: number
  volumeEnvelope: KeyframeTrack
}

export interface AudioMixState {
  tracks: AudioTrackSpec[]
  dynamics: AudioDynamicsState
}

export function defaultAudioDynamics(): AudioDynamicsState {
  return {
    denoise: 0,
    dereverb: false,
    compressor: null,
    limiter: null,
    gate: null,
    deesser: false,
    highpassHz: null,
    lowpassHz: null,
    bitrateKbps: DEFAULT_BITRATE_KBPS,
    volumeEnvelope: [],
  }
}

function defaults(): AudioMixState {
  return { tracks: [], dynamics: defaultAudioDynamics() }
}

export const audioMixState = reactive<AudioMixState>(defaults())

export function resetAudioMix(): void {
  audioMixState.tracks = []
  audioMixState.dynamics = defaultAudioDynamics()
}

function sanitizeDucking(value: unknown): DuckingSpec | null {
  if (!isRecord(value)) return null
  return {
    enabled: boolOr(value.enabled, true),
    threshold: clampNumber(value.threshold, 0, 1, 0.05),
    ratio: clampNumber(value.ratio, 1, 20, 8),
    attack: clampNumber(value.attack, 0.01, 2000, 20),
    release: clampNumber(value.release, 0.01, 9000, 300),
  }
}

/** Validate an untrusted audio track; null when it has no resolvable asset. */
export function sanitizeAudioTrack(value: unknown): AudioTrackSpec | null {
  if (!isRecord(value)) return null
  const assetId = assetIdOr(value.assetId, null)
  if (!assetId) return null
  return {
    assetId,
    role: enumOr(value.role, AUDIO_ROLES, 'music'),
    gain: clampNumber(value.gain, 0, 4, 1),
    start: clampNumber(value.start, 0, MAX_TIMELINE_SECONDS, 0),
    sourceStart: clampNumber(value.sourceStart, 0, MAX_TIMELINE_SECONDS, 0),
    end: clampNullable(value.end, 0, MAX_TIMELINE_SECONDS),
    loop: boolOr(value.loop, false),
    fadeIn: clampNumber(value.fadeIn, 0, MAX_FADE_SECONDS, 0),
    fadeOut: clampNumber(value.fadeOut, 0, MAX_FADE_SECONDS, 0),
    ducking: sanitizeDucking(value.ducking),
  }
}

function serializeAudioTrack(track: AudioTrackSpec): Record<string, unknown> {
  const out: Record<string, unknown> = {
    assetId: track.assetId,
    role: enumOr(track.role, AUDIO_ROLES, 'music'),
  }
  const gain = clampNumber(track.gain, 0, 4, 1)
  if (gain !== 1) out.gain = gain
  const start = clampNumber(track.start, 0, MAX_TIMELINE_SECONDS, 0)
  if (start !== 0) out.start = start
  const sourceStart = clampNumber(track.sourceStart, 0, MAX_TIMELINE_SECONDS, 0)
  if (sourceStart !== 0) out.sourceStart = sourceStart
  const end = clampNullable(track.end, 0, MAX_TIMELINE_SECONDS)
  if (end !== null) out.end = end
  if (track.loop) out.loop = true
  const fadeIn = clampNumber(track.fadeIn, 0, MAX_FADE_SECONDS, 0)
  if (fadeIn > 0) out.fadeIn = fadeIn
  const fadeOut = clampNumber(track.fadeOut, 0, MAX_FADE_SECONDS, 0)
  if (fadeOut > 0) out.fadeOut = fadeOut
  const ducking = sanitizeDucking(track.ducking)
  if (ducking?.enabled) out.ducking = { ...ducking }
  return out
}

function sanitizeCompressor(value: unknown): CompressorSpec | null {
  if (!isRecord(value)) return null
  return {
    threshold: clampNumber(value.threshold, -60, 0, -18),
    ratio: clampNumber(value.ratio, 1, 20, 3),
    attack: clampNumber(value.attack, 0.01, 2000, 20),
    release: clampNumber(value.release, 0.01, 9000, 250),
    makeup: clampNumber(value.makeup, 1, 64, 1),
  }
}

function sanitizeLimiter(value: unknown): LimiterSpec | null {
  if (!isRecord(value)) return null
  return { ceiling: clampNumber(value.ceiling, -30, 0, -1) }
}

function sanitizeGate(value: unknown): GateSpec | null {
  if (!isRecord(value)) return null
  return {
    threshold: clampNumber(value.threshold, -90, 0, -45),
    ratio: clampNumber(value.ratio, 1, 20, 2),
  }
}

export function sanitizeAudioDynamics(value: unknown): AudioDynamicsState {
  const source = isRecord(value) ? value : {}
  return {
    denoise: clampNumber(source.denoise, 0, 1, 0),
    dereverb: boolOr(source.dereverb, false),
    compressor: sanitizeCompressor(source.compressor),
    limiter: sanitizeLimiter(source.limiter),
    gate: sanitizeGate(source.gate),
    deesser: boolOr(source.deesser, false),
    highpassHz: clampNullable(source.highpassHz, 10, 20000),
    lowpassHz: clampNullable(source.lowpassHz, 10, 20000),
    bitrateKbps: clampInt(source.bitrateKbps, 64, 320, DEFAULT_BITRATE_KBPS),
    volumeEnvelope: sanitizeKeyframeTrack(source.volumeEnvelope, 0, 4),
  }
}

/** Only fields that differ from the neutral chain reach the wire. */
function serializeDynamics(dynamics: AudioDynamicsState): AudioDynamics {
  const out: AudioDynamics = {}
  const denoise = clampNumber(dynamics.denoise, 0, 1, 0)
  if (denoise > 0) out.denoise = denoise
  if (dynamics.dereverb) out.dereverb = true
  const compressor = sanitizeCompressor(dynamics.compressor)
  if (compressor) out.compressor = compressor
  const limiter = sanitizeLimiter(dynamics.limiter)
  if (limiter) out.limiter = limiter
  const gate = sanitizeGate(dynamics.gate)
  if (gate) out.gate = gate
  if (dynamics.deesser) out.deesser = true
  const highpassHz = clampNullable(dynamics.highpassHz, 10, 20000)
  if (highpassHz !== null) out.highpassHz = highpassHz
  const lowpassHz = clampNullable(dynamics.lowpassHz, 10, 20000)
  if (lowpassHz !== null) out.lowpassHz = lowpassHz
  const bitrateKbps = clampInt(dynamics.bitrateKbps, 64, 320, DEFAULT_BITRATE_KBPS)
  if (bitrateKbps !== DEFAULT_BITRATE_KBPS) out.bitrateKbps = bitrateKbps
  const volumeEnvelope = sanitizeKeyframeTrack(dynamics.volumeEnvelope, 0, 4)
  if (volumeEnvelope.length) out.volumeEnvelope = volumeEnvelope
  return out
}

export function audioMixPayload(): Record<string, unknown> {
  const payload: Record<string, unknown> = {}
  const tracks = sanitizeList(audioMixState.tracks, MAX_AUDIO_TRACKS, sanitizeAudioTrack)
  if (tracks.length) payload.audioTracks = tracks.map(serializeAudioTrack)
  const dynamics = serializeDynamics(audioMixState.dynamics)
  if (Object.keys(dynamics).length) payload.audioDynamics = dynamics
  return payload
}

export function applyAudioMixSnapshot(raw: unknown): void {
  const source = isRecord(raw) ? raw : {}
  audioMixState.tracks = sanitizeList(source.tracks, MAX_AUDIO_TRACKS, sanitizeAudioTrack)
  audioMixState.dynamics = sanitizeAudioDynamics(source.dynamics)
}

export function cloneAudioDynamics(dynamics: AudioDynamicsState): AudioDynamicsState {
  return {
    ...dynamics,
    compressor: dynamics.compressor ? { ...dynamics.compressor } : null,
    limiter: dynamics.limiter ? { ...dynamics.limiter } : null,
    gate: dynamics.gate ? { ...dynamics.gate } : null,
    volumeEnvelope: cloneTrack(dynamics.volumeEnvelope),
  }
}

// --- helpers for AudioMixPanel ---
//
// The panel binds to `audioMixState` directly for plain scalars; everything
// that has to stay inside the wire contract (track count, keyframe ticks,
// neutral blocks that must not leak into the payload) goes through here.

export const AUDIO_ROLE_LABELS: Record<AudioRole, string> = {
  music: 'Музыка',
  voiceover: 'Озвучка',
  sfx: 'Эффект',
}

/** Offered bitrates, all inside the 64..320 range the wire accepts. */
export const AUDIO_BITRATE_OPTIONS: readonly number[] = [64, 96, 128, 160, 192, 256, 320]

export const ENVELOPE_MIN_GAIN = 0
export const ENVELOPE_MAX_GAIN = 4

/**
 * The backend keyframe time base is 1 ms and rejects two keyframes that round
 * to the same tick, so envelope times are quantized to milliseconds here and
 * neighbours are kept at least one step apart.
 */
export const ENVELOPE_TIME_STEP = 0.001

/**
 * How the legacy `normalizeAudio` switch and the legacy volume slider interact
 * with this panel. `audio_mix::master_tail_filters` emits `loudnorm` first and
 * `volume` after it, so the slider trims the normalized level instead of being
 * swallowed by it. Keep this text in sync with that function.
 */
export const NORMALIZE_VOLUME_NOTE =
  'Нормализация приводит звук к целевому уровню -14 LUFS, а ползунок «Громкость» ' +
  'применяется после неё и сдвигает уже нормализованный уровень. Огибающая ниже ' +
  'работает раньше и меняет соотношение громких и тихих участков, а не итоговый ' +
  'уровень.'

export const BITRATE_NOTE =
  'Задаёт битрейт звуковой дорожки при экспорте. Выше битрейт: лучше качество и ' +
  'больше файл.'

export function defaultDucking(): DuckingSpec {
  return { enabled: true, threshold: 0.05, ratio: 8, attack: 20, release: 300 }
}

export function defaultCompressor(): CompressorSpec {
  return { threshold: -18, ratio: 3, attack: 20, release: 250, makeup: 1 }
}

export function defaultLimiter(): LimiterSpec {
  return { ceiling: -1 }
}

export function defaultGate(): GateSpec {
  return { threshold: -45, ratio: 2 }
}

/** A neutral extra track: full gain, at the start of the timeline, no ducking. */
export function defaultAudioTrack(assetId: string, role: AudioRole): AudioTrackSpec {
  return {
    assetId,
    role,
    gain: 1,
    start: 0,
    sourceStart: 0,
    end: null,
    loop: false,
    fadeIn: 0,
    fadeOut: 0,
    ducking: role === 'music' ? defaultDucking() : null,
  }
}

/** Append a track. Returns false for an unusable id or a full track list. */
export function addAudioTrack(assetId: unknown, role: AudioRole = 'music'): boolean {
  const id = assetIdOr(assetId, null)
  if (!id) return false
  if (audioMixState.tracks.length >= MAX_AUDIO_TRACKS) return false
  audioMixState.tracks = [
    ...audioMixState.tracks,
    defaultAudioTrack(id, enumOr(role, AUDIO_ROLES, 'music')),
  ]
  return true
}

export function removeAudioTrack(index: number): void {
  if (!Number.isInteger(index) || index < 0 || index >= audioMixState.tracks.length) return
  audioMixState.tracks = audioMixState.tracks.filter((_, position) => position !== index)
}

function quantize(seconds: number): number {
  return Math.round(seconds / ENVELOPE_TIME_STEP) * ENVELOPE_TIME_STEP
}

/** A finite, clamped, millisecond-aligned time on the output timeline. */
export function envelopeTime(value: unknown, duration: number): number {
  const max = Number.isFinite(duration) && duration > 0 ? duration : 0
  return quantize(clampNumber(value, 0, max, 0))
}

export function envelopeGain(value: unknown): number {
  return clampNumber(value, ENVELOPE_MIN_GAIN, ENVELOPE_MAX_GAIN, 1)
}

/**
 * The wire carries `interp` per keyframe, but the backend track holds one
 * interpolation for the whole track and takes it from the first keyframe. The
 * panel therefore exposes a single selector, and every point carries the same
 * value so the snapshot never claims something the render will not do.
 */
export function envelopeInterpolation(track: KeyframeTrack): Interpolation {
  return track[0]?.interp ?? 'linear'
}

export function withEnvelopeInterpolation(
  track: KeyframeTrack,
  interp: Interpolation,
): KeyframeTrack {
  return track.map((point) => ({ ...point, interp }))
}

/**
 * Insert a point, or overwrite the value when one already sits on that tick.
 * A full track (64 points) is returned unchanged.
 */
export function addEnvelopePoint(
  track: KeyframeTrack,
  seconds: number,
  value: number,
  duration: number,
): KeyframeTrack {
  const interp = envelopeInterpolation(track)
  const point: Keyframe = { t: envelopeTime(seconds, duration), v: envelopeGain(value), interp }
  const existing = track.findIndex((candidate) => candidate.t === point.t)
  if (existing >= 0) {
    return track.map((candidate, index) => (index === existing ? point : { ...candidate }))
  }
  if (track.length >= MAX_KEYFRAMES) return track
  return [...track.map((candidate) => ({ ...candidate })), point].sort((a, b) => a.t - b.t)
}

/**
 * Move one point. The time stays strictly between its neighbours so no two
 * keyframes ever collapse onto the same millisecond tick; when there is no room
 * left the point keeps its time and only the value changes.
 */
export function moveEnvelopePoint(
  track: KeyframeTrack,
  index: number,
  seconds: number,
  value: number,
  duration: number,
): KeyframeTrack {
  const current = track[index]
  if (!current) return track
  const lower = track[index - 1] ? track[index - 1]!.t + ENVELOPE_TIME_STEP : 0
  const upper = track[index + 1]
    ? track[index + 1]!.t - ENVELOPE_TIME_STEP
    : Math.max(0, Number.isFinite(duration) ? duration : 0)
  const requested = envelopeTime(seconds, duration)
  const t = lower > upper ? current.t : quantize(Math.max(lower, Math.min(upper, requested)))
  return track.map((point, position) =>
    position === index ? { ...point, t, v: envelopeGain(value) } : { ...point },
  )
}

export function removeEnvelopePoint(track: KeyframeTrack, index: number): KeyframeTrack {
  if (!track[index]) return track
  return track.filter((_, position) => position !== index).map((point) => ({ ...point }))
}

/**
 * Value of the envelope at `seconds`, mirroring the backend sampler: the track
 * holds before the first point and after the last one, and the whole track uses
 * the interpolation of its first keyframe.
 */
export function sampleEnvelope(track: KeyframeTrack, seconds: number): number {
  const first = track[0]
  if (!first) return 1
  const time = Number.isFinite(seconds) ? Math.max(0, seconds) : 0
  if (time <= first.t) return first.v
  const last = track[track.length - 1]!
  if (time >= last.t) return last.v

  let index = 1
  while (index < track.length && track[index]!.t <= time) index += 1
  const left = track[index - 1]!
  const right = track[index]!
  const span = right.t - left.t
  const progress = span > 0 ? (time - left.t) / span : 0
  let eased = progress
  if (envelopeInterpolation(track) === 'hold') {
    eased = 0
  } else if (envelopeInterpolation(track) === 'smooth') {
    eased = progress < 0.5 ? 4 * progress ** 3 : 1 - (-2 * progress + 2) ** 3 / 2
  }
  return left.v + (right.v - left.v) * eased
}
