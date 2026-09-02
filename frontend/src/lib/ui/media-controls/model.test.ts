import { describe, expect, it } from 'vitest'
import {
  commandForMediaKey,
  mediaControlLabel,
  mediaTimeValueText,
  nextMediaControl,
  normalizeMediaCommand,
  type MediaControlState,
} from './model.js'

const state: MediaControlState = { currentTime: 65, duration: 125, muted: false, paused: true, volume: 0.42 }

describe('headless media controls', () => {
  it('provides stable localized labels and aria value text', () => {
    expect(mediaControlLabel('play', state)).toBe('Воспроизвести')
    expect(mediaControlLabel('volume', state)).toBe('Громкость 42%')
    expect(mediaTimeValueText(65, 125)).toBe('1:05 из 2:05')
  })

  it('normalizes keyboard commands against duration and supports roving focus order', () => {
    expect(commandForMediaKey({ key: 'ArrowRight', shiftKey: true })).toEqual({ type: 'seek-relative', seconds: 10 })
    expect(normalizeMediaCommand({ type: 'seek-relative', seconds: 100 }, state)).toEqual({ type: 'seek', seconds: 125 })
    expect(nextMediaControl('play', -1)).toBe('volume')
    expect(nextMediaControl('volume', 1)).toBe('play')
  })
})
