import { expect, it } from 'vitest'
import { rebindNativeLayerReferences } from './nativeLayerReferences'
import type { WireCompositionTrack, WireVideoEffect } from './types'
const lane = (id: string, kind: 'image' | 'video' | 'text' | 'audio' = 'image', effects: readonly WireVideoEffect[] = [], locked = false): WireCompositionTrack => ({ id, kind, name: id, locked, clips: effects.length ? [{ id: `${id}-clip`, effects }] : [] } as unknown as WireCompositionTrack)
it('keeps referenced lane identity across deletion for all dependent effect kinds and preserves opaque fields', () => {
  for (const [kind, field] of [['track_matte', 'matte'], ['gradient_wipe', 'layer'], ['compound_blur', 'layer']] as const) {
    const effect = { kind, parameters: { [field]: 3, ...(kind === 'track_matte' ? { reverse: true } : { invert: true }) } } as WireVideoEffect
    const tracks = [lane('empty'), lane('dependent', 'image', [effect]), lane('text', 'text'), lane('sound', 'audio'), lane('control')]
    const before = JSON.stringify(tracks)
    const result = rebindNativeLayerReferences(tracks, tracks.slice(1))
    const clip = result[0]!.clips[0]!
    expect('effects' in clip && clip.effects?.[0]).toEqual({ ...effect, parameters: { ...('parameters' in effect ? effect.parameters : {}), [field]: 2 } })
    expect(result[1]).toBe(tracks[2]); expect(result[3]).toBe(tracks[4])
    expect(JSON.stringify(tracks)).toBe(before)
  }
})
it('refuses deleting a referenced source and changing locked dependents atomically', () => {
  const dependent = lane('dependent', 'image', [{ kind: 'track_matte', parameters: { matte: 2 } }])
  const control = lane('control')
  expect(() => rebindNativeLayerReferences([dependent, control], [dependent])).toThrow('используется')
  const locked = { ...dependent, locked: true }
  expect(() => rebindNativeLayerReferences([lane('empty'), locked, control], [lane('empty'), locked])).not.toThrow()
  const refersThird = lane('locked', 'image', [{ kind: 'track_matte', parameters: { matte: 3 } }], true)
  expect(() => rebindNativeLayerReferences([lane('empty'), refersThird, control], [refersThird, control])).toThrow('заблокированную')
})
it('leaves self fallback and missing-layer fallback unchanged', () => {
  const track = lane('dependent', 'image', [{ kind: 'track_matte', parameters: { matte: 0 } }, { kind: 'compound_blur', parameters: { layer: 8 } }])
  expect(rebindNativeLayerReferences([lane('empty'), track], [track])[0]).toBe(track)
})
it('refuses activating an unrelated missing source when a new lane is inserted', () => {
  const dependent = lane('dependent', 'image', [{ kind: 'track_matte', parameters: { matte: 2 } }])
  expect(() => rebindNativeLayerReferences([dependent], [lane('new'), dependent])).toThrow('отсутствовавшую')
})
