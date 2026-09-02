export type PlayerEventName =
  | 'durationchange'
  | 'ended'
  | 'error'
  | 'loadedmetadata'
  | 'pause'
  | 'play'
  | 'playing'
  | 'timeupdate'
  | 'waiting'

export interface PlayerSnapshot {
  readonly currentTime: number
  readonly duration: number
  readonly muted: boolean
  readonly paused: boolean
  readonly playbackRate: number
  readonly volume: number
}

export interface PlayerAdapter {
  snapshot(): PlayerSnapshot
  play(): Promise<void>
  pause(): void
  seek(seconds: number): void
  setMuted(muted: boolean): void
  setPlaybackRate(rate: number): void
  setVolume(volume: number): void
  load(): void
  subscribe(event: PlayerEventName, listener: () => void): () => void
}

export function clampPlayerTime(seconds: number, duration: number): number {
  if (!Number.isFinite(seconds)) return 0
  const upper = Number.isFinite(duration) && duration > 0 ? duration : Number.MAX_SAFE_INTEGER
  return Math.min(upper, Math.max(0, seconds))
}

export function clampPlayerVolume(volume: number): number {
  return Number.isFinite(volume) ? Math.min(1, Math.max(0, volume)) : 1
}

export function clampPlaybackRate(rate: number): number {
  return Number.isFinite(rate) && rate > 0 ? Math.min(16, Math.max(0.0625, rate)) : 1
}
