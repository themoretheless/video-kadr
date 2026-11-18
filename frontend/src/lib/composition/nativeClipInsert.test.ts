import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { insertNativeMedia } from './nativeClipInsert'
import { editNativeTrack } from './nativeTrackEdit'
import type { CompositionSource } from './types'
const project = (): NativeNestedProject => structuredClone(fixture.createdTimeline) as unknown as NativeNestedProject
const video: CompositionSource = { id: 'new-source', kind: 'video', width: 4, height: 4, durationTicks: 10000000, hasAudio: true, fps: 25, vcodec: 'qtrle' }
it('inserts video/audio/image with exact clocks and source fields, extending only the target duration', () => {
  for (const kind of ['video', 'audio', 'image'] as const) {
    let input = project()
    if (kind === 'image') input = editNativeTrack(input, 'new-timeline', { kind: 'add', id: 'images', trackKind: 'image', name: 'Images' })
    const before = JSON.stringify(input)
    const source = kind === 'image' ? { ...video, kind, id: 'still', durationTicks: 0, hasAudio: false } : video
    const trackId = kind === 'image' ? 'images' : kind === 'audio' ? 'audio-1' : 'video-1'
    const output = insertNativeMedia(input, 'new-timeline', source, { trackId, clipId: 'inserted', startUs: 2000000, sourceInUs: kind === 'image' ? 0 : 1000000, durationUs: 3000000 })
    const track = output.compositions['new-timeline']!.content.tracks.find(track => track.id === trackId)!
    expect(track.clips[0]).toMatchObject(kind === 'image' ? { timelineStartTick: 2000000, durationTicks: 3000000 } : { placement: { timelineStartTick: 2000000, sourceInTick: 1000000, sourceOutTick: 4000000, speed: 1 } })
    expect(output.compositions['new-timeline']!.content.sources[source.id]).not.toHaveProperty('fps')
    expect(output.compositions['new-timeline']!.content.sources[source.id]).not.toHaveProperty('vcodec')
    expect(output.compositions.root).toBe(input.compositions.root)
    expect(JSON.stringify(input)).toBe(before)
  }
})
it('rejects overlap, locks, duplicate clips, wrong kinds, conflicting sources and invalid ranges atomically', () => {
  const input = project(), request = { trackId: 'video-1', clipId: 'inserted', startUs: 0, sourceInUs: 0, durationUs: 3000000 }
  const added = insertNativeMedia(input, 'new-timeline', video, request), before = JSON.stringify(added)
  expect(() => insertNativeMedia(added, 'new-timeline', video, { ...request, clipId: 'overlap' })).toThrow('уже есть')
  expect(() => insertNativeMedia(added, 'new-timeline', video, { ...request, startUs: 4000000 })).toThrow('идентификаторы')
  expect(() => insertNativeMedia(input, 'new-timeline', video, { ...request, trackId: 'audio-1', sourceInUs: 9000000 })).toThrow('пределы')
  expect(() => insertNativeMedia(input, 'new-timeline', { ...video, kind: 'audio', width: 0, height: 0 }, request)).toThrow('Тип')
  expect(() => insertNativeMedia(added, 'new-timeline', { ...video, width: 8 }, { ...request, clipId: 'different', startUs: 4000000 })).toThrow('Метаданные')
  const locked = editNativeTrack(input, 'new-timeline', { kind: 'flag', id: 'video-1', flag: 'locked', value: true })
  expect(() => insertNativeMedia(locked, 'new-timeline', video, request)).toThrow('заблокирована')
  expect(() => insertNativeMedia(input, 'new-timeline', video, { ...request, startUs: NaN })).toThrow('время')
  expect(JSON.stringify(added)).toBe(before)
})

it('chooses a free bottom lane at the requested time and stages separate video sound in the same transaction', async () => {
  const { insertNativeMediaAtFirstFree } = await import('./nativeClipInsert')
  const input = project(), before = JSON.stringify(input)
  const result = insertNativeMediaAtFirstFree(input, 'new-timeline', video, { clipId: 'auto', newTrackId: 'fresh', startUs: 2000000, sourceInUs: 1000000, durationUs: 3000000 })
  const tracks = result.compositions['new-timeline']!.content.tracks
  expect(tracks[0]!.clips).toHaveLength(0)
  expect(tracks[1]!.clips[0]).toMatchObject({ id: 'auto', placement: { timelineStartTick: 2000000, sourceInTick: 1000000, sourceOutTick: 4000000 }, sourceAudioEnabled: false })
  expect(tracks[3]!.clips[0]).toMatchObject({ id: 'auto-audio', placement: { timelineStartTick: 2000000, sourceInTick: 1000000, sourceOutTick: 4000000 } })
  expect(tracks).toHaveLength(input.compositions['new-timeline']!.content.tracks.length)
  expect(JSON.stringify(input)).toBe(before)
})
it('creates an upper lane when all compatible lanes are occupied, keeps time and refuses failed companion atomically', async () => {
  const { insertNativeMediaAtFirstFree } = await import('./nativeClipInsert')
  let input = project()
  const request = { startUs: 2000000, sourceInUs: 0, durationUs: 3000000, videoAudio: false }
  for (const trackId of ['video-1', 'video-2']) input = insertNativeMedia(input, 'new-timeline', video, { ...request, trackId, clipId: `occupied-${trackId}` })
  const before = JSON.stringify(input)
  const result = insertNativeMediaAtFirstFree(input, 'new-timeline', video, { ...request, clipId: 'auto', newTrackId: 'fresh' })
  expect(result.compositions['new-timeline']!.content.tracks[0]).toMatchObject({ id: 'fresh', kind: 'video', clips: [{ id: 'auto', placement: { timelineStartTick: 2000000 } }] })
  let full = input
  while (full.compositions['new-timeline']!.content.tracks.length < 16) full = editNativeTrack(full, 'new-timeline', { kind: 'add', trackKind: 'color', name: 'C', id: `c-${full.compositions['new-timeline']!.content.tracks.length}` })
  const fullBefore = JSON.stringify(full)
  expect(() => insertNativeMediaAtFirstFree(full, 'new-timeline', video, { ...request, clipId: 'auto', newTrackId: 'fresh' })).toThrow('предел')
  expect(JSON.stringify(full)).toBe(fullBefore); expect(JSON.stringify(input)).toBe(before)
})
it('rolls back staged video when no companion audio lane can be created', async () => {
  const { insertNativeMediaAtFirstFree } = await import('./nativeClipInsert')
  let input = project()
  for (const id of ['audio-1', 'audio-2']) input = editNativeTrack(input, 'new-timeline', { kind: 'flag', id, flag: 'locked', value: true })
  while (input.compositions['new-timeline']!.content.tracks.length < 16) input = editNativeTrack(input, 'new-timeline', { kind: 'add', trackKind: 'color', name: 'C', id: `c-${input.compositions['new-timeline']!.content.tracks.length}` })
  const before = JSON.stringify(input)
  expect(() => insertNativeMediaAtFirstFree(input, 'new-timeline', video, { clipId: 'auto', newTrackId: 'fresh', startUs: 0, sourceInUs: 0, durationUs: 1000000 })).toThrow('предел')
  expect(JSON.stringify(input)).toBe(before)
})
it('rebinds retained control-layer identities when a new automatic lane is created above them', async () => {
  const { insertNativeMediaAtFirstFree } = await import('./nativeClipInsert')
  for (const scenario of fixture.nativeRebinds) {
    const input = scenario.before as unknown as NativeNestedProject
    const before = JSON.stringify(input)
    const result = insertNativeMediaAtFirstFree(input, 'root', video, { clipId: 'auto', newTrackId: 'fresh', startUs: 4000000, sourceInUs: 0, durationUs: 1000000, videoAudio: false })
    expect(result.compositions.root!.content.tracks[0]!.id).toBe('fresh')
    const dependent = result.compositions.root!.content.tracks.find(track => track.id === 'dependent')!.clips[0]!
    const field = scenario.kind === 'track_matte' ? 'matte' : 'layer'
    expect('effects' in dependent && dependent.effects?.[0]).toMatchObject({ kind: scenario.kind, parameters: { [field]: 4 } })
    expect(JSON.stringify(input)).toBe(before)
  }
})

it('keeps the embedded audio preference when inserting silent video', async () => {
  const { insertNativeMediaAtFirstFree } = await import('./nativeClipInsert')
  const silent = { ...video, hasAudio: false }
  const request = { clipId: 'silent', startUs: 2000000, sourceInUs: 0, durationUs: 1000000 }
  for (const videoAudio of [undefined, false, true]) {
    const manual = insertNativeMedia(project(), 'new-timeline', silent, {
      ...request, trackId: 'video-2', videoAudio,
    })
    const automatic = insertNativeMediaAtFirstFree(project(), 'new-timeline', silent, {
      ...request, newTrackId: 'fresh', videoAudio,
    })
    for (const output of [manual, automatic]) {
      const tracks = output.compositions['new-timeline']!.content.tracks
      expect(tracks.reduce((count, track) => count + track.clips.length, 0)).toBe(1)
      const clips = tracks.filter(track => track.kind === 'video').flatMap(track => track.clips)
      expect(clips).toHaveLength(1)
      expect(clips[0]).toMatchObject({ id: 'silent', sourceAudioEnabled: videoAudio ?? true })
    }
  }
})

it('inserts Concat video with source-aware fit and embedded sound', async () => {
  const { insertNativeMediaAtFirstFree } = await import('./nativeClipInsert')
  const result = insertNativeMediaAtFirstFree(project(), 'new-timeline', video, {
    clipId: 'concat', newTrackId: 'fresh', startUs: 2000000, sourceInUs: 0,
    durationUs: 1000000, concatFit: true,
  })
  const content = result.compositions['new-timeline']!.content
  expect(content.tracks.reduce((count, track) => count + track.clips.length, 0)).toBe(1)
  expect(content.tracks[1]!.clips[0]).toMatchObject({
    id: 'concat', independentAudio: true, sourceAudioEnabled: true,
    transform: { scaleX: { value: 1 }, scaleY: { value: 1 },
      concatPreparation: { crop: [0, 0, 0, 0], canvas: [content.canvas.width, content.canvas.height] } },
  })
})

it('uses Concat fit for a still image on a differently sized canvas', () => {
  const image = { ...video, kind: 'image' as const, hasAudio: false, durationTicks: 0, width: 8, height: 8 }
  const input = project()
  const withTrack = editNativeTrack(input, 'new-timeline', { kind: 'add', id: 'still', trackKind: 'image', name: 'Image' })
  const result = insertNativeMedia(withTrack, 'new-timeline', image, {
    trackId: 'still', clipId: 'concat-still', startUs: 0, sourceInUs: 0,
    durationUs: 1000000, concatFit: true,
  })
  const content = result.compositions['new-timeline']!.content
  expect(content.tracks.find(track => track.id === 'still')?.clips[0]).toMatchObject({
    id: 'concat-still', durationTicks: 1000000, transform: {
      scaleX: { value: 1 }, scaleY: { value: 1 },
      concatPreparation: { crop: [0, 0, 0, 0], canvas: [content.canvas.width, content.canvas.height] },
    },
  })
})

it('converts an existing clip scale to Concat coordinates without resetting its pose', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const constant = (value: number) => ({ mode: 'constant' as const, value })
  const before = {
    x: constant(12), y: constant(-6), rotationDegrees: constant(15),
    anchorX: 0.5, anchorY: 0.5, scaleX: constant(4), scaleY: constant(2),
  }
  expect(applyConcatFit(before, {width: 100, height: 50}, {width: 25, height: 25}))
    .toEqual({...before, scaleX: constant(2), scaleY: constant(1),
      concatPreparation: {crop: [0, 0, 0, 0], canvas: [100, 50]} })
  expect(before.scaleX).toEqual(constant(4))
  expect(() => applyConcatFit({...before, scaleX: constant(40)},
    {width: 100, height: 50}, {width: 25, height: 25})).toThrow('Масштаб')
})

it('converts synchronized linear scale keys to a Concat scale run', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const constant = (value: number) => ({ mode: 'constant' as const, value })
  const keys = (first: number, last: number) => ({mode: 'keyframes' as const, track: {
    timeBase: 1_000_000, interpolation: 'linear' as const,
    keyframes: [{tick: 0, value: first}, {tick: 500_000, value: last}],
  }})
  const before = {
    x: constant(12), y: constant(-6), rotationDegrees: constant(15),
    anchorX: 0.5, anchorY: 0.5, scaleX: keys(2, 4), scaleY: keys(1, 2),
  }
  const options = {durationTicks: 1_000_000, opacity: constant(0.8)}
  const result = applyConcatFit(before, {width:100,height:50}, {width:25,height:25}, options)
  expect(result.concatAnimation).toMatchObject({
    keys:{scale:[{at:0,value:1},{at:0.5,value:2}]},
    stretch:[1,0.5], rest:{scaleX:1,scaleY:1,x:0.12,y:-0.12,rotationDegrees:15,opacity:0.8},
  })
  expect(result.scaleX).toEqual(constant(1))
  expect(result.scaleY).toEqual(constant(0.5))
  const withMiddleKey = (value: number) => ({mode: 'keyframes' as const, track: {
    ...before.scaleY.track,
    keyframes: [{tick: 0, value: 1}, {tick: 250_000, value}, {tick: 500_000, value: 2}],
  }})
  expect(applyConcatFit({...before, scaleY: withMiddleKey(1.5)}, {width:100,height:50},
    {width:25,height:25}, options)).toEqual(result)
  expect(() => applyConcatFit({...before, scaleY: withMiddleKey(1.6)}, {width:100,height:50},
    {width:25,height:25}, options)).toThrow('пропорции')
  expect(() => applyConcatFit({...before, scaleY: keys(1,3)}, {width:100,height:50},
    {width:25,height:25}, options)).toThrow('пропорции')
})

it('converts independent linear position, rotation and opacity runs with static scale', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const constant = (value: number) => ({ mode: 'constant' as const, value })
  const linear = (tick: number, value: number) => ({mode: 'keyframes' as const, track: {
    timeBase: 1_000_000, interpolation: 'linear' as const,
    keyframes: [{tick: 0, value: 0}, {tick, value}],
  }})
  const before = {
    x: linear(500_000, 25), y: linear(250_000, -10), rotationDegrees: linear(1_000_000, 90),
    anchorX: 0.5, anchorY: 0.5, scaleX: constant(2), scaleY: constant(1),
  }
  const options = {durationTicks: 1_000_000, opacity: linear(750_000, 0.75)}
  const result = applyConcatFit(before, {width:100,height:50}, {width:25,height:25}, options)
  const key = (at: number, value: number) => ({at, value, ease: [0, 0, 1, 1]})
  expect(result.concatAnimation?.keys).toMatchObject({
    offsetX: [key(0, 0), key(0.5, 0.25)], offsetY: [key(0, 0), key(0.25, -0.2)],
    rotation: [key(0, 0), key(1, 90)], opacity: [key(0, 0), key(0.75, 0.75)],
  })
  expect(result.concatAnimation?.keys.scale).toBeUndefined()
  expect(result.concatAnimation?.stretch).toEqual([1, 0.5])
  expect(() => applyConcatFit({...before, x: linear(500_000, 325)},
    {width:100,height:50}, {width:25,height:25}, options)).toThrow('пределы Concat')
})

it('preserves outgoing Bézier tangents and cubic easing on Concat incoming keys', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleBezier } = await import('./bezier')
  const constant = (value: number) => ({ mode: 'constant' as const, value })
  const free = (x: number, y: number) => ({x,y,mode:'free' as const})
  const curve = {kind:'bezier' as const,p1:[0.2,0.1] as const,p2:[0.7,0.9] as const}
  const applied = {...curve,p1:[0.3,0.15] as const,p2:[0.8,0.95] as const}
  const transform = {
    scaleX:constant(1), scaleY:constant(1), anchorX:0.5, anchorY:0.5,
    x:{mode:'keyframes' as const,track:{timeBase:1_000_000,interpolation:curve,keyframes:[
      {tick:0,value:0,tangents:{incoming:free(0,0),outgoing:free(...applied.p1),continuity:'broken' as const}},
      {tick:1_000_000,value:80,tangents:{incoming:free(...applied.p2),outgoing:free(1,1),continuity:'broken' as const}},
    ]}},
    y:constant(0),
    rotationDegrees:{mode:'keyframes' as const,track:{timeBase:1_000_000,interpolation:'ease_in_out' as const,
      keyframes:[{tick:0,value:0},{tick:1_000_000,value:90}]}},
  }
  const size = {width:100,height:100}
  const converted = applyConcatFit(transform,size,size,{durationTicks:1_000_000,opacity:constant(1)})
  const keys = converted.concatAnimation!.keys
  expect(keys.offsetX?.[1]?.ease).toEqual([0.3,0.15,0.8,0.95])
  expect(keys.rotation).toHaveLength(3)
  expect(keys.rotation?.[1]).toMatchObject({at:0.5,value:45})
  for (const time of [0.1,0.25,0.49,0.5,0.51,0.75,0.9]) {
    expect(sampleConcatSourceRun(keys.offsetX!,time)).toBeCloseTo(0.8*sampleBezier(applied,time),6)
    const expected = time < 0.5 ? 4*time**3 : 1-4*(1-time)**3
    expect(sampleConcatSourceRun(keys.rotation!,time)).toBeCloseTo(90*expected,6)
  }
})

it.each(['bounce_in', 'bounce_out', 'bounce_in_out'] as const)('preserves %s between editable Concat keys', async interpolation => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const,track:{timeBase:1_000_000,interpolation,keyframes:[
    {tick:100_000,value:80},{tick:900_000,value:10},
  ]}}
  const size = {width:100,height:100}
  const converted = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks:1_000_000,opacity:constant(1)})
  const keys = converted.concatAnimation!.keys.offsetX!
  expect(keys).toHaveLength(interpolation === 'bounce_in_out' ? 15 : 8)
  for (let tick = 0; tick <= 1_000_000; tick += 1000) {
    expect(sampleConcatSourceRun(keys,tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,9)
  }
})

it('retains continued slopes before and after the authored Concat keys', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const, track:{
    timeBase:1_000_000,
    interpolation:{kind:'bezier' as const,p1:[0.2,0.1] as const,p2:[0.8,0.7] as const},
    extrapolation:{before:'continue' as const,after:'continue' as const},
    keyframes:[{tick:250_000,value:20},{tick:750_000,value:40}],
  }}
  const size = {width:100,height:100}
  const converted = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks:1_000_000,opacity:constant(1)})
  const keys = converted.concatAnimation!.keys.offsetX!
  expect(keys.map(key => key.at)).toEqual([0,0.25,0.75,1])
  expect(converted.concatAnimation?.rest.x).toBe(0.2)
  for (let tick = 0; tick <= 1_000_000; tick += 1000) {
    expect(sampleConcatSourceRun(keys,tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,6)
  }
})

it('exports clipped ping-pong tails as reversed Bézier runs', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const, track:{
    timeBase:1_000_000,
    interpolation:{kind:'bezier' as const,p1:[0.2,0.1] as const,p2:[0.8,0.7] as const},
    extrapolation:{before:'ping_pong' as const,after:'ping_pong' as const},
    keyframes:[{tick:220_000,value:20},{tick:370_000,value:40},{tick:510_000,value:30}],
  }}
  const size = {width:100,height:100}
  const converted = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks:1_000_000,opacity:constant(1)})
  const keys = converted.concatAnimation!.keys.offsetX!
  expect(keys[0]!.at).toBe(0)
  expect(keys.at(-1)!.at).toBe(1)
  expect(keys.some(key => Math.abs(key.at - 0.65) < 1e-9 && key.value === 0.4)).toBe(true)
  for (let tick = 0; tick <= 1_000_000; tick += 1000) {
    expect(sampleConcatSourceRun(keys,tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,6)
  }
})

it.each(['loop','offset'] as const)('carries continuous %s cycles into editable Concat runs', async mode => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const, track:{
    timeBase:1_000_000,
    interpolation:{kind:'bezier' as const,p1:[0.2,0.1] as const,p2:[0.8,0.7] as const},
    extrapolation:{before:mode,after:mode},
    keyframes:[{tick:220_000,value:20},{tick:370_000,value:40},{tick:510_000,value:mode==='loop'?20:30}],
  }}
  const size = {width:100,height:100}
  const transform = {
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  }
  const options = {durationTicks:1_000_000,opacity:constant(1)}
  const keys = applyConcatFit(transform,size,size,options).concatAnimation!.keys.offsetX!
  expect(keys[0]!.at).toBe(0)
  expect(keys.at(-1)!.at).toBe(1)
  for (let tick = 0; tick <= 1_000_000; tick += 1000) {
    expect(sampleConcatSourceRun(keys,tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,6)
  }
  if (mode === 'loop') {
    const discontinuous = {...x, track:{...x.track,keyframes:[{tick:220_000,value:20},{tick:510_000,value:30}]}}
    const jump = applyConcatFit({...transform,x:discontinuous},size,size,options).concatAnimation!.keys.offsetX!
    for (const seam of [220_000, 510_000, 800_000]) {
      for (let tick = seam - 3; tick <= seam + 3; tick++) {
        expect(sampleConcatSourceRun(jump,tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(discontinuous,tick)/100,6)
      }
    }
  }
})

it('keeps hold steps at the exact native microsecond, even between adjacent keys', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const,track:{
    timeBase:1_000_000,interpolation:'hold' as const,
    keyframes:[{tick:200_000,value:0},{tick:350_000,value:25},
      {tick:350_001,value:40,interpolation:'linear' as const},{tick:700_000,value:60}],
  }}
  const size = {width:100,height:100}
  const keys = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks:1_000_000,opacity:constant(1)}).concatAnimation!.keys.offsetX!
  expect(keys.map(key => key.at)).toEqual([0.2,0.349999,0.35,0.350001,0.7])
  for (let tick = 349_990; tick <= 350_010; tick++) {
    expect(sampleConcatSourceRun(keys,tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,9)
  }
  for (let tick = 0; tick <= 1_000_000; tick += 1000) {
    expect(sampleConcatSourceRun(keys,tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,9)
  }
})

it('keeps the authored last value for a one-microsecond loop', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const size = {width:100,height:100}
  const keys = applyConcatFit({
    x:{mode:'keyframes',track:{timeBase:1_000_000,interpolation:'linear',
      extrapolation:{before:'loop',after:'loop'},
      keyframes:[{tick:3,value:20},{tick:4,value:80}]}},
    y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks:8,opacity:constant(1)}).concatAnimation!.keys.offsetX!
  for (let tick = 0; tick <= 8; tick++) {
    expect(sampleConcatSourceRun(keys,tick/8)).toBeCloseTo(tick === 4 ? 0.8 : 0.2, 9)
  }
})

it.each([
  'ease_in_out_cubic',
  {kind:'bezier',p1:[0.9,-0.4],p2:[0.1,1.4]},
] as const)('retains editable Concat gain easing for %j', async interpolation => {
  const { convertConcatGain } = await import('./concatPreparation')
  const { prepareConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const channel = {mode:'keyframes' as const,track:{
    timeBase:1_000_000,interpolation,keyframes:[{tick:0,value:0.5},{tick:1_000_000,value:1.5}],
  }}
  const {keys} = convertConcatGain(channel,1_000_000)
  expect(keys).toHaveLength(typeof interpolation === 'string' ? 3 : 2)
  expect(keys.at(-1)?.ease).toEqual(typeof interpolation === 'string'
    ? [1/3, 1, 2/3, 1] : [...interpolation.p1, ...interpolation.p2])
  expect(keys[0]).toEqual({at:0,value:0.5})
  expect(keys.at(-1)).toMatchObject({at:1,value:1.5})
  const sample = prepareConcatSourceRun(keys)
  let maximumError = 0
  for (let tick = 0; tick <= 1_000_000; tick += 100) {
    maximumError = Math.max(maximumError, Math.abs(sample(tick/1_000_000)-sampleAnimatableValue(channel,tick)))
  }
  expect(maximumError).toBeLessThanOrEqual(0.0001)
})

it.each(['in','out','in_out'] as const)('keeps Elastic %s within the stated Concat error bound', async dir => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { sampleConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const,track:{
    timeBase:1_000_000,
    interpolation:{kind:'elastic' as const,dir,amplitude:1.5,period:0.27},
    keyframes:[{tick:0,value:50},{tick:1_000_000,value:150}],
  }}
  const size = {width:100,height:100}
  const keys = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks:1_000_000,opacity:constant(1)}).concatAnimation!.keys.offsetX!
  expect(keys.length).toBeLessThan(4096)
  expect(keys.at(-1)).toMatchObject({at:1,value:1.5})
  let maximumError = 0
  for (const tick of [1, 999_999, ...Array.from({length:10_001},(_,i)=>i*100)]) {
    maximumError = Math.max(maximumError, Math.abs(
      sampleConcatSourceRun(keys,tick/1_000_000) - sampleAnimatableValue(x,tick)/100,
    ))
  }
  expect(maximumError).toBeLessThanOrEqual(0.0001)
})

it('exports a fractional frame clock with a signed phase on the local microsecond grid', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { prepareConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const { keyframeAuthoredTick } = await import('./keyframeClock')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const,track:{
    timeBase:1,interpolation:'ease_in_out' as const,
    sampleClock:{rateNum:24000, rateDen:1001, offsetNum:-1_500_000_000},
    keyframes:[{tick:-2,value:0},{tick:12,value:80}],
  }}
  const size = {width:100,height:100}
  const durationTicks = 500_000
  const keys = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks,opacity:constant(1)}).concatAnimation!.keys.offsetX!
  const sample = prepareConcatSourceRun(keys)
  const transitions = Array.from({length:11}, (_,frame) =>
    Math.ceil(((frame - 0.5) * 1001 + 1500) * 1_000_000 / 24000))
  for (const tick of transitions) {
    expect(keyframeAuthoredTick(x.track,tick) - keyframeAuthoredTick(x.track,tick-1)).toBe(1)
  }
  for (const tick of [0,durationTicks,...transitions.flatMap(t=>[t-1,t,t+1])]) {
    expect(sample(tick/durationTicks)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,9)
  }
})

it('skips long constant tails around a short animation on a frame clock', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { prepareConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const,track:{
    timeBase:1,interpolation:'linear' as const,sampleClock:{rateNum:24,rateDen:1,offsetNum:0},
    keyframes:[{tick:4800,value:20},{tick:4810,value:80}],
  }}
  const size = {width:100,height:100}
  const durationTicks = 600_000_000
  const keys = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks,opacity:constant(1)}).concatAnimation!.keys.offsetX!
  expect(keys).toHaveLength(21)
  const sample = prepareConcatSourceRun(keys)
  for (const tick of [0, 199_000_000, 200_000_000, 200_020_833, 200_020_834, 200_100_000, 200_416_667, durationTicks]) {
    expect(sample(tick/durationTicks)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,9)
  }
  const outside = {...x,track:{...x.track,keyframes:[{tick:14_999,value:10},{tick:15_000,value:20}]}}
  const beforeRun = applyConcatFit({
    x:outside,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks,opacity:constant(1)}).concatAnimation!.keys.offsetX!
  expect(beforeRun).toMatchObject([{at:0,value:0.1}])
})

it('exports the visible subcurve when both authored Bézier keys are outside the clip', async () => {
  const { applyConcatFit } = await import('./concatPreparation')
  const { prepareConcatSourceRun } = await import('./concatSourceWindow')
  const { sampleAnimatableValue } = await import('./keyframes')
  const constant = (value: number) => ({mode:'constant' as const,value})
  const x = {mode:'keyframes' as const,track:{
    timeBase:1_000_000,
    interpolation:{kind:'bezier' as const,p1:[0.3,0.12] as const,p2:[0.8,0.7] as const},
    keyframes:[{tick:-220_000,value:10},{tick:1_310_000,value:80}],
  }}
  const size = {width:100,height:100}
  const result = applyConcatFit({
    x,y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),
    anchorX:0.5,anchorY:0.5,
  },size,size,{durationTicks:1_000_000,opacity:constant(1)})
  const keys = result.concatAnimation!.keys.offsetX!
  expect(keys.map(key=>key.at)).toEqual([0,1])
  const sample = prepareConcatSourceRun(keys)
  for (let tick = 0; tick <= 1_000_000; tick+=1000) {
    expect(sample(tick/1_000_000)).toBeCloseTo(sampleAnimatableValue(x,tick)/100,6)
  }
})
