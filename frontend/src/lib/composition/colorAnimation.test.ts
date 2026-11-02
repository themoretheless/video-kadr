import { afterEach,expect,it } from 'vitest'
import { addClip,addTrack,createComposition,duplicateClip,splitClip,trimClip,registerSource } from './commands'
import { buildCompositionRenderRequest } from './payload'
import { configureTextColor,deleteTextColorKey,setTextColorKey,setTextColorTangentMode,editTextColorTangents,moveTextColorKeys } from './colorCommands'
import { packColorAnimation,nativeColorToHex,normalizeColorAnimation } from './colorAnimation'
import { assertValidComposition,normalizeComposition,compositionRenderUnavailableReason } from './validation'
import { commitDocument,compositionState,resetCompositionForTests,undoComposition } from '../state/composition.svelte'
import type { Composition,CompositionAnimatableColor,TextClip } from './types'

function document(colorAnimation?: CompositionAnimatableColor) {
  let value = createComposition({ width: 128,height: 128,fps: 20,backgroundColor: '#000000' })
  value = addTrack(value,{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] })
  return addClip(value,'t',{ id: 'c',kind: 'text',timelineStartTicks: 1_000_000,durationTicks: 2_000_000,text: 'Color',x: 0,y: 0,opacity: 1,style: { fontSizePx: 24,color: '#FF0000FF',align: 'center' },...(colorAnimation ? { colorAnimation } : {}) })
}
const clip = (value: Composition,index = 0) => value.tracks[0]!.clips[index] as TextClip
afterEach(resetCompositionForTests)

it('exports authored RGBA keys through the strict native text wire', () => {
  const animation: CompositionAnimatableColor = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',extrapolation: { before: 'ping_pong',after: 'offset' },keyframes: [{ tick: -1,value: '#FF000080' },{ tick: 1000,value: '#0000FFFF' }] } }
  let value = registerSource(document(animation),{ id: 'v',kind: 'video',durationTicks: 4_000_000,width: 128,height: 128,hasAudio: false })
  value = addTrack(value,{ id: 'video',kind: 'video',name: 'Video',hidden: false,muted: false,locked: false,clips: [] })
  value = addClip(value,'video',{ id: 'primary',kind: 'video',sourceId: 'v',timelineStartTicks: 0,sourceInTicks: 0,sourceOutTicks: 4_000_000,transform: { x: 0,y: 0,width: 128,height: 128,fit: 'contain' },opacity: 1,sourceAudioEnabled: false,audioGain: 1 })
  expect(compositionRenderUnavailableReason(value)).toBeNull()
  const wire = buildCompositionRenderRequest(value).composition.tracks.find(track => track.kind === 'text')
  expect(wire?.clips[0]).toMatchObject({ colorAnimation: packColorAnimation(animation) })
})

it('solves color Auto to identity and freezes Free without scalar slopes or Smooth coupling', () => {
  const value: CompositionAnimatableColor = { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [0.25,0.1],p2: [0.75,0.9] },keyframes: [{ tick: 0,value: '#FF000080' },{ tick: 500,value: '#00FF00FF' },{ tick: 2000,value: '#0000FF00' }] } }
  const original = document(value),before = JSON.stringify(original)
  const auto = setTextColorTangentMode(original,'c',500,'auto')
  const packed = packColorAnimation(clip(auto).colorAnimation!)
  expect(packed.keys[1]!.in_).toEqual([2/3,2/3])
  expect(packed.keys[1]!.out).toEqual([1/3,1/3])
  expect(packed.keys[0]!.out).toEqual([0.25,0.1])
  expect(packed.keys[2]!.in_).toEqual([0.75,0.9])
  const free = setTextColorTangentMode(auto,'c',500,'free')
  expect(packColorAnimation(clip(free).colorAnimation!)).toEqual({...packed,keys:packed.keys.map((key,index)=>index===1?{...key,tangent_modes:{incoming:'free',outgoing:'free',continuity:'smooth'}}:key)})
  expect(packed.keys[1]!.tangent_modes).toEqual({incoming:'auto',outgoing:'auto',continuity:'smooth'})
  expect(clip(free).colorAnimation).toMatchObject({ track: { keyframes: [{ tick: 0 },{ tangents: { incoming: { mode: 'free' },outgoing: { mode: 'free' },continuity: 'smooth' } },{ tick: 2000 }] } })
  const smooth: CompositionAnimatableColor = { ...value,track: { ...value.track,keyframes: value.track.keyframes.map(key => key.tick !== 500 ? key : { ...key,tangents: { incoming: { x: 0.9,y: -0.2,mode: 'free' },outgoing: { x: 0.2,y: 0.4,mode: 'free' },continuity: 'smooth' } }) } }
  expect(normalizeColorAnimation(smooth)).toEqual(smooth)
  const staleAuto: CompositionAnimatableColor = { ...smooth,track: { ...smooth.track,keyframes: smooth.track.keyframes.map(key => key.tick !== 500 ? key : { ...key,tangents: { incoming: { x: 0.9,y: -0.2,mode: 'auto' },outgoing: { x: 0.2,y: 0.4,mode: 'auto' },continuity: 'smooth' } }) } }
  const raw = document(staleAuto),rawBytes = JSON.stringify(raw)
  const loaded = normalizeComposition(JSON.parse(rawBytes))
  expect(packColorAnimation(clip(loaded).colorAnimation!).keys[1]!.in_).toEqual([2/3,2/3])
  expect(packColorAnimation(clip(loaded).colorAnimation!).keys[1]!.out).toEqual([1/3,1/3])
  expect(normalizeComposition(loaded)).toEqual(loaded)
  expect(JSON.stringify(raw)).toBe(rawBytes)
  expect(clip(normalizeComposition(document(smooth))).colorAnimation).toEqual(smooth)
  const manual = editTextColorTangents(document(smooth),'c',500,{ side: 'outgoing',point: { x: 0.42,y: -0.4 } })
  const manualPacked = packColorAnimation(clip(manual).colorAnimation!)
  expect(manualPacked.keys[1]!.out).toEqual([0.42,-0.4])
  expect(manualPacked.keys[1]!.in_).toEqual([0.9,-0.2])
  const broken = editTextColorTangents(manual,'c',500,{ continuity: 'broken' })
  expect(packColorAnimation(clip(broken).colorAnimation!)).toEqual({...manualPacked,keys:manualPacked.keys.map((key,index)=>index===1?{...key,tangent_modes:{incoming:'free',outgoing:'free',continuity:'broken'}}:key)})
  expect(() => editTextColorTangents(original,'c',500,{ side: 'incoming',point: { x: 2,y: 0 } })).toThrow()
  expect(JSON.stringify(original)).toBe(before)
})

it('bakes color timing presets without a scalar slope and retains per-segment overrides', () => {
  const original = document({ mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: '#FF000080' },{ tick: 1000,value: '#00FF00FF',interpolation: 'hold' },{ tick: 2000,value: '#00000000' }] } })
  const before = JSON.stringify(original)
  const changed = configureTextColor(original,'c',{ preset: 'ease_in_quad',side: 'after',extrapolation: 'offset' })
  const value = clip(changed).colorAnimation!
  const packed = packColorAnimation(value)
  expect(packed.keys[0]!.out).toEqual([1/3,0])
  expect(packed.keys[1]!.in_).toEqual([2/3,1/3])
  expect(packed.keys[1]!.segment).toEqual({ kind: 'Hold' })
  expect(packed.keys.map(key => key.value)).toEqual([{ r: 255,g: 0,b: 0,a: 128 },{ r: 0,g: 255,b: 0,a: 255 },{ r: 0,g: 0,b: 0,a: 0 }])
  expect(packed.extrapolate).toEqual({ before: 'Hold',after: 'Offset' })
  const keyed = configureTextColor(changed,'c',{ tick: 1000,preset: 'ease_out_elastic' })
  expect(packColorAnimation(clip(keyed).colorAnimation!).keys[1]!.segment).toEqual({ kind: 'Elastic',dir: 'Out',amplitude: 1,period: 0.3 })
  expect(JSON.stringify(original)).toBe(before)
  expect(() => configureTextColor(original,'c',{ preset: 'missing' })).toThrow('Unknown')
  expect(() => configureTextColor(original,'c',{ tick: 7,preset: 'linear' })).toThrow('exist')
  const custom = configureTextColor(original,'c',{ tick: 0,interpolation: { kind: 'bezier',p1: [0.2,-0.3],p2: [0.8,1.4] } })
  expect(packColorAnimation(clip(custom).colorAnimation!).keys[0]!.out).toEqual([0.2,-0.3])
  expect(packColorAnimation(clip(custom).colorAnimation!).keys[1]!.in_).toEqual([0.8,1.4])
  expect(() => configureTextColor(original,'c',{ interpolation: { kind: 'bezier',p1: [2,0],p2: [0.8,1] } })).toThrow()
})

it('packs color geometry independently of scalar values and preserves fractional clocks and legacy easing', () => {
  const value: CompositionAnimatableColor = { mode: 'keyframes',track: { timeBase: 30,interpolation: 'ease_in_out',extrapolation: { before: 'continue',after: 'ping_pong' },keyframes: [{ tick: -1,value: '#FF000080' },{ tick: 1,value: '#0000FF',interpolation: { kind: 'bezier',p1: [0.2,-0.3],p2: [0.8,1.3] },tangents: { incoming: { x: 0.7,y: 1,mode: 'free' },outgoing: { x: 0.4,y: -0.2,mode: 'free' },continuity: 'broken' } },{ tick: 3,value: '#00000000' }] } }
  const before = JSON.stringify(value),packed = packColorAnimation(value)
  expect(packed.extrapolate).toEqual({ before: 'Continue',after: 'PingPong' })
  expect(packed.keys[0]).toMatchObject({ t_us: -1_000_000/30,value: { r: 255,g: 0,b: 0,a: 128 },legacy_curve: 'ease_in_out',segment: { kind: 'Spline' },out: [1/3,0] })
  expect(packed.keys[1]).toMatchObject({ t_us: 1_000_000/30,in_: [2/3,1],out: [0.4,-0.2],value: { r: 0,g: 0,b: 255,a: 255 } })
  expect(packed.keys[2]!.in_).toEqual([0.8,1.3])
  expect(JSON.stringify(value)).toBe(before)
  expect(nativeColorToHex({ r: 0,g: 255,b: 16,a: 128 })).toBe('#00ff1080')
  expect(() => nativeColorToHex({ r: 0.5,g: 0,b: 0,a: 255 })).toThrow()
})

it('authors color keys in local clock with replacement, collapse and exact Undo', () => {
  resetCompositionForTests()
  commitDocument(document())
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  commitDocument(setTextColorKey(compositionState.document,'c',1_500_000,'#00FF0080'))
  expect(clip(compositionState.document).colorAnimation).toEqual({ mode: 'keyframes',track: { timeBase: 1_000_000,interpolation: 'linear',keyframes: [{ tick: 0,value: '#FF0000FF' },{ tick: 500000,value: '#00FF0080' }] } })
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
  const first = setTextColorKey(document(),'c',1_500_000,'#00FF0080')
  const replaced = setTextColorKey(first,'c',1_500_000,'#0000FFFF')
  expect(clip(replaced).colorAnimation).toMatchObject({ track: { keyframes: [{ value: '#FF0000FF' },{ value: '#0000FFFF' }] } })
  const one = deleteTextColorKey(replaced,'c',0)
  expect(clip(deleteTextColorKey(one,'c',500000)).colorAnimation).toEqual({ mode: 'constant',value: '#0000FFFF' })
  expect(() => setTextColorKey(first,'c',999999,'#000000')).toThrow('outside')
  expect(() => setTextColorKey(first,'c',1_500_000,'#GG0000')).toThrow()
  const locked = { ...first,tracks: first.tracks.map(track => ({ ...track,locked: true })) }
  expect(() => setTextColorKey(locked,'c',1_500_000,'#FFFFFF')).toThrow('locked')
})

it('retains full color curve and clock through serialization, split, trim and detached duplication', () => {
  const animated: CompositionAnimatableColor = { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [0.2,-0.1],p2: [0.8,1.2] },extrapolation: { before: 'offset',after: 'ping_pong' },keyframes: [{ tick: 0,value: '#FF0000FF',tangents: { incoming: { x: 2/3,y: 2/3,mode: 'auto' },outgoing: { x: 0.2,y: -0.1,mode: 'free' },continuity: 'broken' } },{ tick: 1000,value: '#00FF0080' },{ tick: 2000,value: '#0000FF00' }] } }
  const original = document(animated),before = JSON.stringify(original)
  expect(clip(normalizeComposition(JSON.parse(before))).colorAnimation).toEqual(animated)
  const halves = splitClip(original,'c',2_000_000,'right')
  expect(clip(halves).colorAnimation).toEqual(animated)
  expect(clip(halves,1).colorAnimation).toMatchObject({ track: { timeBase: 1000,extrapolation: animated.track.extrapolation,keyframes: [{ tick: -1000,value: '#FF0000FF' },{ tick: 0,value: '#00FF0080' },{ tick: 1000,value: '#0000FF00' }] } })
  const trimmed = trimClip(original,'c',1_250_000,2_750_000)
  expect(clip(trimmed).colorAnimation).toMatchObject({ track: { keyframes: [{ tick: -250 },{ tick: 750 },{ tick: 1750 }] } })
  const copy = duplicateClip(original,'c',{ id: 'copy',timelineStartTicks: 3_000_000 })
  const copied = clip(copy,1).colorAnimation
  if (copied?.mode !== 'keyframes' || typeof copied.track.interpolation !== 'object' || copied.track.interpolation.kind !== 'bezier') throw new Error('Bezier color track required')
  ;(copied.track.interpolation.p1 as unknown as number[])[1] = 9
  ;(copied.track.keyframes[0]!.tangents!.outgoing as { y: number }).y = 9
  expect(JSON.stringify(original)).toBe(before)
  assertValidComposition(halves);assertValidComposition(trimmed)
  expect(compositionRenderUnavailableReason(original)).toBeNull()
  const invalid = JSON.parse(before);invalid.tracks[0].clips[0].colorAnimation.track.keyframes[1].value = 42
  expect(() => assertValidComposition(invalid)).toThrow()
  const duplicateTime = JSON.parse(before);duplicateTime.tracks[0].clips[0].colorAnimation.track.keyframes[1].tick = 0
  expect(() => assertValidComposition(duplicateTime)).toThrow()
  const dense = JSON.parse(before)
  const denseKeys = Array.from({ length: 4096 },(_,tick) => ({ tick,value: '#FF000080' }))
  dense.tracks[0].clips[0].colorAnimation.track.keyframes = denseKeys
  assertValidComposition(dense)
  denseKeys.push({ tick: 4096,value: '#FF000080' })
  expect(() => assertValidComposition(dense)).toThrow('4096')
})

it('moves multiple color keys atomically including swaps and rejects collisions', () => {
  const source = document({ mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: -100,value: '#FF000080' },{ tick: 200,value: '#0000FFFF' },{ tick: 400,value: '#00FF00FF' }] } })
  const before = JSON.stringify(source)
  const moved = moveTextColorKeys(source,[{ clipId: 'c',tick: -100,newTick: 200 },{ clipId: 'c',tick: 200,newTick: -100 }])
  expect(clip(moved).colorAnimation).toMatchObject({ track: { keyframes: [{ tick: -100,value: '#0000FFFF' },{ tick: 200,value: '#FF000080' },{ tick: 400,value: '#00FF00FF' }] } })
  expect(() => moveTextColorKeys(source,[{ clipId: 'c',tick: -100,newTick: 200 }])).toThrow()
  expect(() => moveTextColorKeys(source,[{ clipId: 'c',tick: -100,newTick: 0 },{ clipId: 'c',tick: -100,newTick: 1 }])).toThrow('Duplicate')
  expect(() => moveTextColorKeys(source,[{ clipId: 'c',tick: -100,newTick: 0 },{ clipId: 'missing',tick: 200,newTick: 300 }])).toThrow()
  expect(JSON.stringify(source)).toBe(before)
})
