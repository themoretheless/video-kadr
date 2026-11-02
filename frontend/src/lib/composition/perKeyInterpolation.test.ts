import { beforeEach, expect, it } from 'vitest'
import { sampleAnimatableValue, upsertKeyframe, updateKeyframe, sliceAnimatableValue } from './keyframes'
import { createComposition, addTrack, addClip, registerSource, setClipLocked } from './commands'
import { buildCompositionRenderRequest } from './payload'
import { validateComposition } from './validation'
import { compositionState, resetCompositionForTests, selectCompositionKeyframe, setCompositionKeyframeSelectionInterpolation, undoComposition } from '../state/composition.svelte'
import type { CompositionAnimatableValue } from './types'
const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0, interpolation: 'ease_in' }, { tick: 1_000_000, value: 10, interpolation: 'hold' }, { tick: 2_000_000, value: 20 }] } }
beforeEach(() => resetCompositionForTests())
it('uses left-key easing, preserves legacy fallback and retains overrides through key edits and slices', () => {
  expect(sampleAnimatableValue(value, 500_000)).toBe(1.25)
  expect(sampleAnimatableValue(value, 1_500_000)).toBe(10)
  expect(upsertKeyframe(value, 0, 1, 0)).toMatchObject({ track: { keyframes: [{ tick: 0, interpolation: 'ease_in' }, { tick: 1_000_000, interpolation: 'hold' }, { tick: 2_000_000 }] } })
  expect(updateKeyframe(value, 0, 100_000, 0)).toMatchObject({ track: { keyframes: [{ tick: 100_000, interpolation: 'ease_in' }, { tick: 1_000_000, interpolation: 'hold' }, { tick: 2_000_000 }] } })
  expect(sliceAnimatableValue(value, 0, 2_000_000, 2_000_000)).toEqual(value)
})
it('applies selected-key easing across properties once, roundtrips render wire and refuses a locked participant', () => {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const [id, start] of [['a', 0], ['b', 3_000_000]] as const) {
    document = addTrack(document, { id, kind: 'text', name: id, locked: false, hidden: false, clips: [] })
    document = addClip(document, id, { id: `${id}-clip`, kind: 'text', text: id, timelineStartTicks: start, durationTicks: 2_000_000, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' }, animation: { x: value } })
  }
  document = registerSource(document, { id: 'video', kind: 'video', durationTicks: 6_000_000, width: 1280, height: 720, hasAudio: false })
  document = addTrack(document, { id: 'primary', kind: 'video', name: 'Primary', locked: false, hidden: false, muted: false, clips: [] })
  document = addClip(document, 'primary', { id: 'primary-clip', kind: 'video', sourceId: 'video', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 6_000_000, transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
  compositionState.document = document
  selectCompositionKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 0)
  selectCompositionKeyframe({ kind: 'visual', clipId: 'b-clip' }, 'x', 1_000_000, true)
  const subjects = compositionState.ui.selectedKeyframes.map((subject) => ({ ...subject, ticks: [...subject.ticks] }))
  const depth = compositionState.history.past.length
  expect(() => setCompositionKeyframeSelectionInterpolation('hold', [subjects[0]!, subjects[0]!])).toThrow('Duplicate keyframe subjects')
  expect(compositionState.document).toEqual(document)
  expect(compositionState.history.past).toHaveLength(depth)
  setCompositionKeyframeSelectionInterpolation('ease_out')
  expect(compositionState.history.past).toHaveLength(depth + 1)
  expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ animation: { x: { track: { keyframes: [{ tick: 0, interpolation: 'ease_out' }, { tick: 1_000_000, interpolation: 'hold' }, { tick: 2_000_000 }] } } } })
  expect(validateComposition(compositionState.document)).toEqual([])
  expect(JSON.stringify(buildCompositionRenderRequest(compositionState.document))).toContain('"interpolation":"ease_out"')
  undoComposition()
  expect(compositionState.document).toEqual(document)
  compositionState.document = setClipLocked(document, 'b-clip', true)
  const locked = JSON.stringify(compositionState.document)
  expect(() => setCompositionKeyframeSelectionInterpolation('hold', subjects)).toThrow(/locked|заблокирован/i)
  expect(JSON.stringify(compositionState.document)).toBe(locked)
  expect(compositionState.history.past).toHaveLength(depth)
})
