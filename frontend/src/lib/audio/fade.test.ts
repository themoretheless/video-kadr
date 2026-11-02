import { expect, it } from 'vitest'
import { audioFadeMultiplier } from './fade'
it('multiplies both envelopes in the overlap, instead of taking their minimum', () => {
  expect(audioFadeMultiplier(500_000, 1_000_000, 800_000, 800_000)).toBe(0.390625)
  expect(audioFadeMultiplier(200_000, 1_000_000, 800_000, 800_000)).toBe(0.25)
  expect(audioFadeMultiplier(800_000, 1_000_000, 800_000, 800_000)).toBe(0.25)
})
it('keeps absent fades at unity and clamps faded boundaries and source handles', () => {
  expect(audioFadeMultiplier(500, 1000)).toBe(1)
  expect(audioFadeMultiplier(0, 1000, 1000, 1000)).toBe(0)
  expect(audioFadeMultiplier(1000, 1000, 1000, 1000)).toBe(0)
  expect(audioFadeMultiplier(-100, 1000, 500, 500)).toBe(0)
  expect(audioFadeMultiplier(1100, 1000, 500, 500)).toBe(0)
})
