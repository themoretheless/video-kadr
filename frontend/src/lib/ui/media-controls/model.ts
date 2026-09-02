import { clampPlayerTime, clampPlayerVolume } from '$lib/ports/player.js'

export interface MediaControlState {
  readonly currentTime: number
  readonly duration: number
  readonly muted: boolean
  readonly paused: boolean
  readonly volume: number
}

export type MediaControlCommand =
  | { readonly type: 'toggle-play' }
  | { readonly type: 'seek'; readonly seconds: number }
  | { readonly type: 'seek-relative'; readonly seconds: number }
  | { readonly type: 'set-volume'; readonly volume: number }
  | { readonly type: 'toggle-mute' }

export const MEDIA_CONTROL_ORDER = ['play', 'seek', 'mute', 'volume'] as const
export type MediaControlId = typeof MEDIA_CONTROL_ORDER[number]

export function formatMediaTime(seconds: number): string {
  const safe = Math.max(0, Number.isFinite(seconds) ? seconds : 0)
  const hours = Math.floor(safe / 3600)
  const minutes = Math.floor((safe % 3600) / 60)
  const secs = Math.floor(safe % 60).toString().padStart(2, '0')
  return hours > 0 ? `${hours}:${minutes.toString().padStart(2, '0')}:${secs}` : `${minutes}:${secs}`
}

export function mediaTimeValueText(currentTime: number, duration: number): string {
  return `${formatMediaTime(currentTime)} из ${formatMediaTime(duration)}`
}

export function mediaControlLabel(id: MediaControlId, state: MediaControlState): string {
  switch (id) {
    case 'play': return state.paused ? 'Воспроизвести' : 'Пауза'
    case 'seek': return `Позиция ${mediaTimeValueText(state.currentTime, state.duration)}`
    case 'mute': return state.muted ? 'Включить звук' : 'Выключить звук'
    case 'volume': return `Громкость ${Math.round(clampPlayerVolume(state.volume) * 100)}%`
  }
}

export function normalizeMediaCommand(command: MediaControlCommand, state: MediaControlState): MediaControlCommand {
  if (command.type === 'seek') {
    return { type: 'seek', seconds: clampPlayerTime(command.seconds, state.duration) }
  }
  if (command.type === 'seek-relative') {
    return { type: 'seek', seconds: clampPlayerTime(state.currentTime + command.seconds, state.duration) }
  }
  if (command.type === 'set-volume') {
    return { type: 'set-volume', volume: clampPlayerVolume(command.volume) }
  }
  return command
}

export function commandForMediaKey(event: Pick<KeyboardEvent, 'key' | 'shiftKey'>): MediaControlCommand | null {
  const step = event.shiftKey ? 10 : 5
  switch (event.key) {
    case ' ':
    case 'k': return { type: 'toggle-play' }
    case 'ArrowLeft': return { type: 'seek-relative', seconds: -step }
    case 'ArrowRight': return { type: 'seek-relative', seconds: step }
    case 'm': return { type: 'toggle-mute' }
    default: return null
  }
}

export function nextMediaControl(current: MediaControlId, direction: -1 | 1): MediaControlId {
  const index = MEDIA_CONTROL_ORDER.indexOf(current)
  return MEDIA_CONTROL_ORDER[(index + direction + MEDIA_CONTROL_ORDER.length) % MEDIA_CONTROL_ORDER.length]!
}
