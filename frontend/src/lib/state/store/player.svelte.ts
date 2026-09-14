import {
  state
} from './core.svelte.js'

export function seekTo(t: number, timelineSegmentId: string | null = null): void {
  const max = state.video?.duration ?? t
  state.seekTimelineSegmentId = timelineSegmentId
  state.seekTo = Math.max(0, Math.min(t, max))
}


export function seekRelative(delta: number): void {
  seekTo(state.playerTime + delta)
}


export function togglePlay(): void {
  state.playToggle++
}


export function setTrimStartFromPlayer(): void {
  if (!state.video) return
  state.edit.trimStart = Math.max(0, Math.min(state.playerTime, state.edit.trimEnd - 0.1))
}


export function setTrimEndFromPlayer(): void {
  if (!state.video) return
  state.edit.trimEnd = Math.min(state.video.duration, Math.max(state.playerTime, state.edit.trimStart + 0.1))
}

// --- single-source timeline ---

