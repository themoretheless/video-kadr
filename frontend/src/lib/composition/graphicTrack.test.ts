import { expect, it } from 'vitest'
import { createComposition, addTrack, addClip, splitClip, trimClip, duplicateClip } from './commands'
import { assertValidComposition, normalizeComposition, compositionRenderUnavailableReason } from './validation'
import { buildCompositionRenderRequest } from './payload'
import type { GraphicClip } from './types'

it('preserves source-free graphic layers and their local clock through editing and native transport', () => {
  let document = createComposition({ width: 128, height: 64, fps: 20, backgroundColor: '#000000' })
  document = addTrack(document, { id: 'graphics', kind: 'graphic', name: 'Graphics', hidden: false, locked: false, clips: [] })
  const clip: GraphicClip = { id: 'title', kind: 'graphic', timelineStartTicks: 500000, durationTicks: 1000000,
    width: 128, height: 64, animationOffsetUs: 100000, roll:{mode:'crawlLeft',startOffScreen:true,endOffScreen:true,preroll:10000,easeIn:20000,easeOut:30000,postroll:40000}, responsiveTime: {intro:100000,outro:200000,designIn:-500000,designDuration:1000000}, graphic: { layers: [{ uid: 1, enabled: true, pin: {to:'frame',left:true,top:false,right:true,bottom:false,offsets:[-4.5,2,7.25,0]}, content: { kind: 'shape', parameters: {}, animation: {opacity:{mode:'keyframes',track:{timeBase:3,interpolation:'hold',keyframes:[{tick:0,value:0},{tick:2,value:100}]}}} } }] },
    transform: { x: 0, y: 0, width: 128, height: 64, fit: 'contain' }, opacity: 1 }
  document = addClip(document, 'graphics', clip)
  document = normalizeComposition(document)
  assertValidComposition(document)
  expect(normalizeComposition(JSON.parse(JSON.stringify(document)))).toEqual(document)
  expect(compositionRenderUnavailableReason(document)).toBeNull()
  const split = splitClip(document, 'title', 1000000, 'right')
  expect((split.tracks[0]!.clips[1] as GraphicClip).animationOffsetUs).toBe(600000)
  const trimmed = trimClip(document, 'title', 750000, 1250000)
  expect((trimmed.tracks[0]!.clips[0] as GraphicClip).animationOffsetUs).toBe(350000)
  const copy = duplicateClip(document, 'title', { id: 'copy', timelineStartTicks: 2000000 })
  expect((copy.tracks[0]!.clips[1] as GraphicClip).graphic).not.toBe(clip.graphic)
  expect((copy.tracks[0]!.clips[1] as GraphicClip).graphic.layers[0]!.pin).toEqual(clip.graphic.layers[0]!.pin)
  expect((copy.tracks[0]!.clips[1] as GraphicClip).graphic.layers[0]!.pin).not.toBe(clip.graphic.layers[0]!.pin)
  expect((split.tracks[0]!.clips[1] as GraphicClip).graphic.layers[0]!.pin).toEqual(clip.graphic.layers[0]!.pin)
  const wire = buildCompositionRenderRequest(document).composition
  expect(wire.sources).toEqual({})
  expect(wire.tracks[0]).toMatchObject({ kind: 'graphic', clips: [{ timelineStartTick: 500000, timelineEndTick: 1500000, animationOffsetUs: 100000, responsiveTime: clip.responsiveTime, roll:clip.roll, graphic: clip.graphic, effects: [] }] })
  expect(wire.tracks[0]?.clips[0]).not.toHaveProperty('sourceId')
})
