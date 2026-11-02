import { describe, expect, it } from 'vitest'
import { createComposition } from './commands'
import { normalizeComposition, validateComposition } from './validation'
import { normalizeVisualScaleLink, scaleTracksTwins, setVisualScaleLinked } from './scaleLink'
import type { CompositionAnimatableValue, TextClip } from './types'

const document = createComposition({ width: 1920, height: 1080, fps: 30, backgroundColor: '#000000' })
const keys: CompositionAnimatableValue = { mode: 'keyframes', track: {
  timeBase: 1_000_000, interpolation: 'linear', keyframes: [
    { tick: 0, value: 1, interpolation: 'ease_in' }, { tick: 1_000_000, value: 2 },
  ],
} }
const clip: TextClip = { id: 'text', kind: 'text', text: 'Title', x: 0, y: 0, opacity: 1,
  timelineStartTicks: 0, durationTicks: 2_000_000,
  style: { fontSizePx: 24, color: '#ffffff', align: 'center' },
  animation: { scaleX: keys, scaleY: { mode: 'constant', value: 3 } },
}

describe('scale track link', () => {
  it('copies the full X shape without aliasing and preserves tracks on unlink', () => {
    const linked = setVisualScaleLinked(document, clip, true)
    expect(linked.animation?.scaleY).toEqual(keys)
    expect(linked.animation?.scaleY).not.toBe(keys)
    if (linked.animation?.scaleY?.mode !== 'keyframes' || keys.mode !== 'keyframes') throw new Error('keyed copy required')
    expect(linked.animation.scaleY.track.keyframes[0]).not.toBe(keys.track.keyframes[0])
    expect(setVisualScaleLinked(document, linked, false).animation).toBe(linked.animation)
    expect(clip.animation?.scaleY).toEqual({ mode: 'constant', value: 3 })
  })

  it('detects easing and time-base divergence even when sampled endpoints match', () => {
    expect(scaleTracksTwins(keys, structuredClone(keys))).toBe(true)
    if (keys.mode !== 'keyframes') throw new Error('keyed fixture required')
    expect(scaleTracksTwins(keys, { ...keys, track: { ...keys.track, timeBase: 25 } })).toBe(false)
    expect(scaleTracksTwins(keys, { ...keys, track: { ...keys.track, keyframes: keys.track.keyframes.map((key) => ({ ...key, interpolation: 'hold' })) } })).toBe(false)
  })

  it('normalizes divergent restored links and keeps the final twin result linked', () => {
    const linked = setVisualScaleLinked(document, clip, true)
    expect(normalizeVisualScaleLink(document, linked)).toBe(linked)
    const divergent = { ...linked, animation: { ...linked.animation, scaleY: { mode: 'constant' as const, value: 4 } } }
    const project = { ...document, tracks: [{ id: 'titles', kind: 'text' as const, name: 'Titles', hidden: false, locked: false, clips: [divergent] }] }
    expect(normalizeComposition(JSON.parse(JSON.stringify(project))).tracks[0]!.clips[0]).toMatchObject({ scaleLinked: false })
    const restored = { ...project, tracks: [{ ...project.tracks[0]!, clips: [linked] }] }
    expect(normalizeComposition(restored).tracks[0]!.clips[0]).toMatchObject({ scaleLinked: true })
    expect(validateComposition({ ...restored, tracks: [{ ...restored.tracks[0]!, clips: [{ ...linked, scaleLinked: 'yes' }] }] }).some((issue) => issue.code === 'scale-linked')).toBe(true)
  })
})
