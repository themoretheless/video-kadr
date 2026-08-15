export type TeleprompterPhase = 'idle' | 'running' | 'paused' | 'finished'

export interface TeleprompterState {
  text: string
  speedPxPerSecond: number
  fontSizePx: number
  phase: TeleprompterPhase
  offsetPx: number
  lastFrameMs: number | null
}

export function createTeleprompterState(): TeleprompterState {
  return {
    text: '',
    speedPxPerSecond: 45,
    fontSizePx: 32,
    phase: 'idle',
    offsetPx: 0,
    lastFrameMs: null,
  }
}

export function configureTeleprompter(
  state: TeleprompterState,
  patch: Partial<Pick<TeleprompterState, 'text' | 'speedPxPerSecond' | 'fontSizePx'>>,
): TeleprompterState {
  return {
    ...state,
    ...patch,
    speedPxPerSecond: Math.min(240, Math.max(10, patch.speedPxPerSecond ?? state.speedPxPerSecond)),
    fontSizePx: Math.min(80, Math.max(16, patch.fontSizePx ?? state.fontSizePx)),
  }
}

export function startTeleprompter(state: TeleprompterState, nowMs: number): TeleprompterState {
  if (!state.text.trim()) return state
  return {
    ...state,
    phase: 'running',
    offsetPx: state.phase === 'finished' ? 0 : state.offsetPx,
    lastFrameMs: nowMs,
  }
}

export function pauseTeleprompter(state: TeleprompterState, nowMs: number, maxOffsetPx: number): TeleprompterState {
  const advanced = advanceTeleprompter(state, nowMs, maxOffsetPx)
  return advanced.phase === 'finished' ? advanced : { ...advanced, phase: 'paused', lastFrameMs: null }
}

export function resetTeleprompter(state: TeleprompterState): TeleprompterState {
  return { ...state, phase: 'idle', offsetPx: 0, lastFrameMs: null }
}

export function advanceTeleprompter(
  state: TeleprompterState,
  nowMs: number,
  maxOffsetPx: number,
): TeleprompterState {
  if (state.phase !== 'running') return state
  const elapsedMs = state.lastFrameMs === null ? 0 : Math.max(0, nowMs - state.lastFrameMs)
  const nextOffset = state.offsetPx + (elapsedMs / 1000) * state.speedPxPerSecond
  const limit = Math.max(0, maxOffsetPx)
  if (nextOffset >= limit && limit > 0) {
    return { ...state, phase: 'finished', offsetPx: limit, lastFrameMs: null }
  }
  return { ...state, offsetPx: nextOffset, lastFrameMs: nowMs }
}
