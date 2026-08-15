import { describe, expect, it } from 'vitest'
import {
  advanceTeleprompter,
  configureTeleprompter,
  createTeleprompterState,
  pauseTeleprompter,
  resetTeleprompter,
  startTeleprompter,
} from './teleprompter.js'

describe('teleprompter model', () => {
  it('starts only with text and advances using elapsed time and speed', () => {
    const empty = createTeleprompterState()
    expect(startTeleprompter(empty, 1000)).toEqual(empty)

    const configured = configureTeleprompter(empty, { text: 'Local script', speedPxPerSecond: 60 })
    const started = startTeleprompter(configured, 1000)
    const advanced = advanceTeleprompter(started, 2500, 500)
    expect(advanced.phase).toBe('running')
    expect(advanced.offsetPx).toBe(90)
  })

  it('pauses without losing position and finishes at the exact bound', () => {
    const configured = configureTeleprompter(createTeleprompterState(), { text: 'Script', speedPxPerSecond: 100 })
    const started = startTeleprompter(configured, 0)
    const paused = pauseTeleprompter(started, 500, 1000)
    expect(paused).toMatchObject({ phase: 'paused', offsetPx: 50, lastFrameMs: null })

    const resumed = startTeleprompter(paused, 1000)
    const finished = advanceTeleprompter(resumed, 3000, 180)
    expect(finished).toMatchObject({ phase: 'finished', offsetPx: 180, lastFrameMs: null })
    expect(resetTeleprompter(finished)).toMatchObject({ phase: 'idle', offsetPx: 0 })
  })

  it('clamps speed and font size to usable local controls', () => {
    const state = configureTeleprompter(createTeleprompterState(), {
      speedPxPerSecond: 1000,
      fontSizePx: 4,
    })
    expect(state.speedPxPerSecond).toBe(240)
    expect(state.fontSizePx).toBe(16)
  })
})
