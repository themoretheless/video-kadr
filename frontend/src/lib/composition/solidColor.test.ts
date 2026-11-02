import { expect,it } from 'vitest'
import { createSolidColorClip,createSolidColorParams,editSolidColorParams,solidColorParamsToWire } from './solidColor'
import { createComposition,addTrack,addClip,registerSource,splitClip,trimClip,duplicateClip } from './commands'
import { assertValidComposition,normalizeComposition,compositionRenderUnavailableReason } from './validation'
import { buildCompositionRenderRequest } from './payload'
import type { ColorClip } from './types'
import type { CompositionAnimatableColor } from './types'

it('defaults to upstream black at canvas size and preserves odd-sized RGBA rectangles', () => {
  const original = createSolidColorParams({ width: 128,height: 64 })
  expect(original).toEqual({ width: 128,height: 64,color: '#000000ff' })
  const changed = editSolidColorParams(original,{ width: 63,height: 31,color: '#12345680' })
  expect(solidColorParamsToWire(changed)).toEqual({ width: 63,height: 31,color: { r: 18,g: 52,b: 86,a: 128 } })
  expect(original.width).toBe(128)
  for (const patch of [{ width: 0 },{ height: 2161 },{ width: 1.5 },{ color: '#gg0000' }]) expect(() => editSolidColorParams(original,patch)).toThrow()
})

it('detaches signed color keys and preserves curves/extrapolation in native wire', () => {
  const animation: CompositionAnimatableColor = { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [.2,-.3],p2: [.8,1.4] },extrapolation: { before: 'ping_pong',after: 'offset' },keyframes: [{ tick: -1,value: '#ff000080' },{ tick: 1000,value: '#0000ffff' }] } }
  const original = createSolidColorParams({ width: 128,height: 64 },{ colorAnimation: animation })
  expect(original.colorAnimation).not.toBe(animation)
  const wire = solidColorParamsToWire(original)
  expect(wire.colorAnimation).toMatchObject({ extrapolate: { before: 'PingPong',after: 'Offset' },keys: [{ t_us: -1000,value: { r: 255,g: 0,b: 0,a: 128 },out: [.2,-.3] },{ t_us: 1000000,value: { b: 255,a: 255 },in_: [.8,1.4] }] })
  if (animation.mode !== 'keyframes' || typeof animation.track.interpolation !== 'object' || animation.track.interpolation.kind !== 'bezier') throw new Error('Bezier fixture required')
  ;(animation.track.interpolation.p1 as unknown as number[])[1] = 99
  expect(solidColorParamsToWire(original)).toEqual(wire)
  const invalid = structuredClone(original)
  if (invalid.colorAnimation?.mode !== 'keyframes') throw new Error('Keyed fixture required')
  ;(invalid.colorAnimation.track.keyframes[1] as { tick: number }).tick = -1
  expect(() => editSolidColorParams(original,{ colorAnimation: invalid.colorAnimation })).toThrow()
  expect(solidColorParamsToWire(editSolidColorParams(original,{ colorAnimation: undefined }))).not.toHaveProperty('colorAnimation')
})

it('persists real color tracks, rebases split/trim keys and emits native color wire', () => {
  const animation: CompositionAnimatableColor = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: '#ff000080' },{ tick: 1000,value: '#0000ffff' }] } }
  let document = createComposition({ width: 128,height: 64,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 'colors',kind: 'color',name: 'Color',hidden: false,locked: false,clips: [] })
  document = addClip(document,'colors',createSolidColorClip(document.canvas,'solid',500000,1000000,{ width: 63,height: 31,colorAnimation: animation }))
  document = normalizeComposition(document)
  const before = JSON.stringify(document)
  assertValidComposition(document)
  expect(normalizeComposition(JSON.parse(before))).toEqual(document)
  const split = splitClip(document,'solid',1000000,'right')
  expect((split.tracks[0]!.clips[1] as ColorClip).colorAnimation).toMatchObject({ track: { keyframes: [{ tick: -500 },{ tick: 500 }] } })
  expect((trimClip(document,'solid',750000,1250000).tracks[0]!.clips[0] as ColorClip).colorAnimation).toMatchObject({ track: { keyframes: [{ tick: -250 },{ tick: 750 }] } })
  const copy = duplicateClip(document,'solid',{ id: 'copy',timelineStartTicks: 2000000 })
  expect((copy.tracks[0]!.clips[1] as ColorClip).colorAnimation).not.toBe((document.tracks[0]!.clips[0] as ColorClip).colorAnimation)
  expect(JSON.stringify(document)).toBe(before)
  expect(compositionRenderUnavailableReason(document)).toBeNull()
  expect(buildCompositionRenderRequest(document).composition.sources).toEqual({})
  document = registerSource(document,{ id: 'video-source',kind: 'video',durationTicks: 2000000,width: 128,height: 64,hasAudio: false })
  document = addTrack(document,{ id: 'video',kind: 'video',name: 'Video',hidden: false,muted: false,locked: false,clips: [] })
  document = addClip(document,'video',{ id: 'primary',kind: 'video',sourceId: 'video-source',timelineStartTicks: 0,sourceInTicks: 0,sourceOutTicks: 2000000,transform: { x: 0,y: 0,width: 128,height: 64,fit: 'contain' },opacity: 1,sourceAudioEnabled: false,audioGain: 1 })
  expect(compositionRenderUnavailableReason(document)).toBeNull()
  const wire = buildCompositionRenderRequest(document).composition.tracks[0]
  expect(wire).toMatchObject({ kind: 'color',clips: [{ width: 63,height: 31,color: { r: 0,g: 0,b: 0,a: 255 },colorAnimation: { keys: [{ value: { r: 255,a: 128 } },{ value: { b: 255,a: 255 } }] },transform: { scaleX: { value: 1 },scaleY: { value: 1 } } }] })
  expect(wire?.clips[0]).not.toHaveProperty('sourceId')
})

it('authors solid color keys with curves, tangents and detached clipboard transport', async () => {
  const { setTextColorKey,configureTextColor,setTextColorTangentMode,moveTextColorKeys } = await import('./colorCommands')
  const { copyColorKeys,pasteColorKeys } = await import('./colorClipboard')
  let document = createComposition({ width: 128,height: 64,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 'colors',kind: 'color',name: 'Color',hidden: false,locked: false,clips: [] })
  document = addClip(document,'colors',createSolidColorClip(document.canvas,'solid',0,2000000))
  const original = document
  document = setTextColorKey(document,'solid',1000000,'#ff000080')
  document = configureTextColor(document,'solid',{ interpolation: { kind: 'bezier',p1: [.2,.1],p2: [.8,.9] } })
  document = setTextColorTangentMode(document,'solid',1000000,'auto')
  const groups = copyColorKeys(document,[{ clipId: 'solid',tick: 1000000 }])
  const pasted = pasteColorKeys(document,groups,['solid'],1500000)
  const moved = moveTextColorKeys(pasted.document,[{ clipId: 'solid',tick: 1500000,newTick: 1750000 }])
  expect((moved.tracks[0]!.clips[0] as ColorClip).colorAnimation).toMatchObject({ track: { keyframes: [{ tick: 0 },{ tick: 1000000 },{ tick: 1750000 }] } })
  expect(pasted.subjects).toEqual([{ clipId: 'solid',tick: 1500000 }])
  expect((pasted.document.tracks[0]!.clips[0] as ColorClip).colorAnimation).toMatchObject({ mode: 'keyframes',track: { keyframes: [{ tick: 0,value: '#000000ff' },{ tick: 1000000,value: '#ff000080' },{ tick: 1500000,value: '#ff000080' }] } })
  expect((original.tracks[0]!.clips[0] as ColorClip).colorAnimation).toBeUndefined()
  expect((document.tracks[0]!.clips[0] as ColorClip).colorAnimation).not.toBe((pasted.document.tracks[0]!.clips[0] as ColorClip).colorAnimation)
})
