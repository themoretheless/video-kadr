import { expect,it,vi } from 'vitest'
import * as api from '../api'
import { insertMotionPathNodeNear,configureMotionPathProgress,arriveAtMotionPathNode,editCompositionMotionPath,setMotionPathProgressKey,switchStaticPositionMode } from './motionPathCommands'
import { addClip,addTrack,createComposition,splitClip,trimClip,duplicateClip } from './commands'
import { sampleAnimatableValue } from './keyframes'
import { assertValidComposition } from './validation'
import { buildCompositionRenderRequest } from './payload'
import type { CompositionPositionAnimation } from './types'

it('preserves path/progress in validated documents and strict export transform', () => {
  const position: CompositionPositionAnimation = { mode: 'path',path: { nodes: [{ id: 'a',point: { x: 0,y: 0 },inHandle: { x: 0,y: 0 },outHandle: { x: 30,y: 40 },segment: 'cubic',tangentMode: 'corner' },{ id: 'b',point: { x: 100,y: 0 },inHandle: { x: -30,y: 40 },outHandle: { x: 0,y: 0 },segment: 'line',tangentMode: 'corner' }] },progress: { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 }] } } }
  let document = createComposition({ width: 128,height: 128,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] })
  document = addClip(document,'t',{ id: 'c',kind: 'text',timelineStartTicks: 0,durationTicks: 1_000_000,text: 'Path',x: 0,y: 0,opacity: 1,style: { fontSizePx: 24,color: '#FFFFFFFF',align: 'center' },position })
  document = { ...document,sources: { media: { id: 'media',kind: 'video',durationTicks: 1_000_000,width: 128,height: 128,hasAudio: false } } }
  document = addTrack(document,{ id: 'base',kind: 'video',name: 'Base',hidden: false,muted: false,locked: false,clips: [] })
  document = addClip(document,'base',{ id: 'base-clip',kind: 'video',sourceId: 'media',timelineStartTicks: 0,sourceInTicks: 0,sourceOutTicks: 1_000_000,transform: { x: 0,y: 0,width: 128,height: 128,fit: 'contain' },opacity: 1,sourceAudioEnabled: false,audioGain: 1 })
  const authoredProgress = (document.tracks[0]!.clips[0] as import('./types').TextClip).position!
  if (authoredProgress.mode !== 'path' || authoredProgress.progress.mode !== 'keyframes') throw new Error('path keys required')
  const enriched = JSON.parse(JSON.stringify(document))
  enriched.tracks[0].clips[0].position.progress.track.interpolation = { kind: 'bezier',p1: [0.2,0.1],p2: [0.8,0.9] }
  enriched.tracks[0].clips[0].position.progress.track.keyframes[0].tangents = { incoming: { x: 0.66,y: 0.66,mode: 'free' },outgoing: { x: 0.33,y: 0.33,mode: 'free' },continuity: 'broken' }
  const snapshot = JSON.stringify(enriched)
  const duplicate = duplicateClip(enriched,'c',{ id: 'copy',timelineStartTicks: 1_000_000 })
  const copied = duplicate.tracks[0]!.clips.find(clip => clip.id === 'copy') as import('./types').TextClip
  if (copied.position?.mode !== 'path') throw new Error('path required')
  ;(copied.position.path.nodes[0]!.point as { x: number }).x = 321
  const enrichedWire = buildCompositionRenderRequest(enriched).composition.tracks[0]!.clips[0] as unknown as { transform: { position: typeof authoredProgress } }
  if (enrichedWire.transform.position.mode !== 'path' || enrichedWire.transform.position.progress.mode !== 'keyframes') throw new Error('path keys required')
  const exportedTrack = enrichedWire.transform.position.progress.track
  if (typeof exportedTrack.interpolation !== 'object' || exportedTrack.interpolation.kind !== 'bezier') throw new Error('bezier required')
  ;(exportedTrack.interpolation.p1 as unknown as number[])[0] = 0.99
  ;(exportedTrack.keyframes[0]!.tangents!.outgoing as { x: number }).x = 0.99
  expect(JSON.stringify(enriched)).toBe(snapshot)
  assertValidComposition(JSON.parse(JSON.stringify(document)))
  const payload = buildCompositionRenderRequest(document)
  const wire = payload.composition.tracks[0]!.clips[0] as unknown as { transform: { position: CompositionPositionAnimation } }
  expect(wire.transform.position).toEqual(position)
  const before = JSON.stringify(document)
  if (wire.transform.position.mode !== 'path') throw new Error('path required')
  ;(wire.transform.position.path.nodes[0]!.point as { x: number }).x = 99
  expect(JSON.stringify(document)).toBe(before)
  const invalid = JSON.parse(before)
  invalid.tracks[0].clips[0].position.path.nodes[1].id = 'a'
  expect(() => assertValidComposition(invalid)).toThrow()
  invalid.tracks[0].clips[0].position = { mode: 'path',x: { mode: 'constant',value: 1 },path: position.path,progress: position.progress }
  expect(() => assertValidComposition(invalid)).toThrow()
})


it('applies native node edits immutably and refuses locks and cancelled responses', async () => {
  let document = createComposition({ width: 128,height: 128,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] })
  const path = { nodes: [{ id: 'a',point: { x: 0,y: 0 },inHandle: { x: 0,y: 0 },outHandle: { x: 0,y: 0 },segment: 'line' as const,tangentMode: 'corner' as const }] }
  document = addClip(document,'t',{ id: 'c',kind: 'text',timelineStartTicks: 0,durationTicks: 1_000_000,text: 'Path',x: 0,y: 0,opacity: 1,style: { fontSizePx: 24,color: '#FFFFFFFF',align: 'center' },position: { mode: 'path',path,progress: { mode: 'constant',value: 0 } } })
  const xy = switchStaticPositionMode(document,'c','xy','unused')
  expect(xy.tracks[0]!.clips[0]).toMatchObject({ position: { mode: 'xy',x: { mode: 'constant',value: 0 },y: { mode: 'constant',value: 0 } } })
  const explicit = JSON.parse(JSON.stringify(xy))
  explicit.tracks[0].clips[0].position.x.value = 77
  explicit.tracks[0].clips[0].position.y.value = -12
  const restored = switchStaticPositionMode(explicit,'c','path','new-node')
  expect(restored.tracks[0]!.clips[0]).toMatchObject({ position: { mode: 'path',path: { nodes: [{ id: 'new-node',point: { x: 77,y: -12 } }] },progress: { mode: 'constant',value: 0 } } })
  explicit.tracks[0].clips[0].position.x = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 }] } }
  const animatedBefore = JSON.stringify(explicit)
  expect(() => switchStaticPositionMode(explicit,'c','path','new-node')).toThrow('motion conversion')
  expect(JSON.stringify(explicit)).toBe(animatedBefore)
  const animatedProgress = JSON.parse(JSON.stringify(document))
  animatedProgress.tracks[0].clips[0].position.progress = explicit.tracks[0].clips[0].position.x
  expect(switchStaticPositionMode(animatedProgress,'c','xy','unused').tracks[0]!.clips[0]).toMatchObject({ position: { mode: 'xy',x: { mode: 'constant',value: 0 } } })
  const keyed = setMotionPathProgressKey(document,'c',500_000,0.75)
  expect(keyed.tracks[0]!.clips[0]).toMatchObject({ position: { path,progress: { mode: 'keyframes',track: { timeBase: 1_000_000,keyframes: [{ tick: 0,value: 0 },{ tick: 500_000,value: 0.75 }] } } } })
  const clock = JSON.parse(JSON.stringify(keyed))
  clock.tracks[0].clips[0].timelineStartTicks = 2_000_000
  clock.tracks[0].clips[0].position.progress.track.timeBase = 1000
  clock.tracks[0].clips[0].position.progress.track.keyframes[1].tick = 500
  const extended = setMotionPathProgressKey(clock,'c',2_750_000,1.1)
  expect(extended.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { keyframes: [{ tick: 0,value: 0 },{ tick: 500,value: 0.75 },{ tick: 750,value: 1.1 }] } } } })
  expect(() => setMotionPathProgressKey(clock,'c',1_999_999,0)).toThrow('outside')
  expect(() => setMotionPathProgressKey(document,'c',1_000_001,0)).toThrow('outside')
  const animated = setMotionPathProgressKey(document,'c',1_000_000,1)
  const eased = configureMotionPathProgress(animated,'c',{ preset: 'ease_in_quad',side: 'after',extrapolation: 'loop' })
  const easedClip = eased.tracks[0]!.clips[0] as import('./types').TextClip
  if (easedClip.position?.mode !== 'path') throw new Error('path required')
  expect(easedClip.position.path).toEqual(path)
  expect(sampleAnimatableValue(easedClip.position.progress,500_000)).toBeCloseTo(0.25,12)
  expect(sampleAnimatableValue(easedClip.position.progress,1_500_000)).toBeCloseTo(0.25,12)
  expect(() => configureMotionPathProgress(animated,'c',{ preset: 'unknown' })).toThrow('Unknown')
  const custom = configureMotionPathProgress(animated,'c',{ interpolation: { kind: 'bezier',p1: [1/3,-0.2],p2: [2/3,1.2] } })
  const customClip = custom.tracks[0]!.clips[0] as import('./types').TextClip
  if (customClip.position?.mode !== 'path') throw new Error('path required')
  expect(customClip.position.path).toEqual(path)
  expect(sampleAnimatableValue(customClip.position.progress,500_000)).toBeCloseTo(0.5,12)
  expect(() => configureMotionPathProgress(animated,'c',{ interpolation: { kind: 'bezier',p1: [2,0],p2: [0.5,1] } })).toThrow()

  const elastic = configureMotionPathProgress(animated,'c',{ interpolation: { kind: 'elastic',dir: 'out',amplitude: 1.5,period: 0.45 } })
  expect(elastic.tracks[0]!.clips[0]).toMatchObject({ position: { path,progress: { track: { interpolation: { kind: 'elastic',amplitude: 1.5,period: 0.45 } } } } })
  expect(() => configureMotionPathProgress(animated,'c',{ interpolation: { kind: 'elastic',dir: 'out',amplitude: 1,period: -0.1 } })).toThrow()
  const halves = splitClip(animated,'c',400_000,'right')
  const left = halves.tracks[0]!.clips[0] as import('./types').TextClip
  const right = halves.tracks[0]!.clips[1] as import('./types').TextClip
  if (left.position?.mode !== 'path' || right.position?.mode !== 'path') throw new Error('path required')
  expect(left.position.path).toEqual(path)
  expect(right.position.path).toEqual(path)
  expect(sampleAnimatableValue(left.position.progress,200_000)).toBeCloseTo(0.2)
  expect(sampleAnimatableValue(right.position.progress,0)).toBeCloseTo(0.4)
  expect(sampleAnimatableValue(right.position.progress,300_000)).toBeCloseTo(0.7)
  const curved = JSON.parse(JSON.stringify(animated))
  curved.tracks[0].clips[0].position.progress.track.interpolation = { kind: 'bezier',p1: [0.2,-0.1],p2: [0.8,1.2] }
  const curvedHalves = splitClip(curved,'c',400_000,'curved-right')
  const curvedRight = curvedHalves.tracks[0]!.clips[1] as import('./types').TextClip
  if (curvedRight.position?.mode !== 'path') throw new Error('path required')
  for (let local = 0;local <= 600_000;local += 10_000) expect(sampleAnimatableValue(curvedRight.position.progress,local)).toBeCloseTo(sampleAnimatableValue(curved.tracks[0].clips[0].position.progress,local+400_000),12)
  const trimmed = trimClip(animated,'c',300_000,900_000)
  const trimmedClip = trimmed.tracks[0]!.clips[0] as import('./types').TextClip
  if (trimmedClip.position?.mode !== 'path') throw new Error('path required')
  expect(trimmedClip.position.path).toEqual(path)
  expect(sampleAnimatableValue(trimmedClip.position.progress,200_000)).toBeCloseTo(0.5)
  const before = JSON.stringify(document)
  const updated = { nodes: [{ ...path.nodes[0]!,point: { x: 12,y: 34 } }] }
  const native = vi.spyOn(api,'evaluateMotionPath').mockResolvedValue({ path: updated,length: 0,points: [] })
  try {
    const changed = await editCompositionMotionPath(document,'c',{ operation: 'edit',index: 0,part: 'point',value: { x: 12,y: 34 } })
    expect(native).toHaveBeenCalledWith({ operation: 'edit',index: 0,part: 'point',value: { x: 12,y: 34 },path },undefined)
    expect(JSON.stringify(document)).toBe(before)
    expect(changed.tracks[0]!.clips[0]).toMatchObject({ position: { path: updated,progress: { mode: 'constant',value: 0 } } })
    const locked = { ...document,tracks: document.tracks.map(track => ({ ...track,locked: true })) }
    await expect(editCompositionMotionPath(locked,'c',{ operation: 'solve' })).rejects.toThrow('locked')
    expect(native).toHaveBeenCalledTimes(1)
    const controller = new AbortController()
    native.mockImplementationOnce(async () => { controller.abort();return { path: updated,length: 0,points: [] } })
    await expect(editCompositionMotionPath(document,'c',{ operation: 'solve' },controller.signal)).rejects.toThrow('cancelled')
    native.mockResolvedValueOnce({ path,length: 0,points: [],nodeProgresses: [0.3141592653589793] })
    const arrived = await arriveAtMotionPathNode(document,'c',0,500_000)
    expect(arrived.tracks[0]!.clips[0]).toMatchObject({ position: { path,progress: { track: { keyframes: [{ tick: 0,value: 0 },{ tick: 500_000,value: 0.3141592653589793 }] } } } })
    expect(native.mock.calls.at(-1)?.[0]).toEqual({ operation: 'measure',path })
    native.mockResolvedValueOnce({ path,length: 100,points: [],nearest: { segment: 0,t: 0.37,distance: 3 } })
    native.mockResolvedValueOnce({ path: updated,length: 100,points: [] })
    const insertedNear = await insertMotionPathNodeNear(document,'c',{ x: 37,y: 3 },'insert',8)
    expect(native.mock.calls.at(-1)?.[0]).toEqual({ operation: 'insert',path,segment: 0,t: 0.37,id: 'insert' })
    expect(insertedNear.tracks[0]!.clips[0]).toMatchObject({ position: { path: updated } })
    native.mockResolvedValueOnce({ path,length: 100,points: [],nearest: { segment: 0,t: 0.37,distance: 9 } })
    await expect(insertMotionPathNodeNear(document,'c',{ x: 37,y: 9 },'too-far',8)).rejects.toThrow('No nearby')

    expect(JSON.stringify(document)).toBe(before)
  } finally { native.mockRestore() }
})
