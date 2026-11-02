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
