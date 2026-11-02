import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { duplicateNativeClips } from './nativeClipDuplicate'
import { editNativeTrack } from './nativeTrackEdit'
it('copies linked authored media after its span with independent curves, names, versions and a new link', () => {
  const value = JSON.parse(JSON.stringify(fixture.nativeVersions[0]!.copy))
  value.compositions['new-timeline'].content.metadata.clipNames = { 'auto-first': 'Camera', 'auto-first-audio': 'Sound' }
  const input = value as NativeNestedProject
  const before = JSON.stringify(input); let serial = 0
  const output = duplicateNativeClips(input, 'new-timeline', ['auto-first'], () => `copy-${serial++}`)
  const node = output.compositions['new-timeline']!
  for (const [index, id] of [[1,'copy-0'],[3,'copy-1']] as const) {
    const old = input.compositions['new-timeline']!.content.tracks[index]!.clips[0]!
    expect(node.content.tracks[index]!.clips[1]).toEqual({ ...old, id, ...('placement' in old ? { placement: { ...old.placement, timelineStartTick: 5000000 } } : {}) })
  }
  expect(node.content.metadata!.clipLinks!['auto-first-link']).toEqual(['auto-first','auto-first-audio'])
  expect(node.content.metadata!.clipLinks!['copy-0-link']).toEqual(['copy-0','copy-1'])
  expect(node.content.metadata!.clipNames!['copy-0']).toBe('Camera')
  expect(node.content.metadata!.mediaVersions!['copy-0']).toEqual(node.content.metadata!.mediaVersions!['auto-first'])
  const old = input.compositions['new-timeline']!.content.tracks[1]!.clips[0]!, copy = node.content.tracks[1]!.clips[1]!
  expect('opacity' in old && 'opacity' in copy && old.opacity !== copy.opacity).toBe(true)
  expect(node.content.sources).toBe(input.compositions['new-timeline']!.content.sources)
  expect(output.compositions.root).toBe(input.compositions.root)
  expect(JSON.stringify(input)).toBe(before)
})
it('refuses locked companions and ID collisions, and resolves occupied target windows', () => {
  const input = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as NativeNestedProject
  const locked = editNativeTrack(input, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  let lockedSerial = 0
  expect(() => duplicateNativeClips(locked, 'new-timeline', ['auto-first'], () => `locked-copy-${lockedSerial++}`)).toThrow('заблокирована')
  expect(() => duplicateNativeClips(input, 'new-timeline', ['auto-first'], () => 'auto-first')).toThrow('идентификатор')
  let serial = 0
  const placed=duplicateNativeClips(input, 'new-timeline', ['auto-first'], () => `copy-${serial++}`, 2500000)
  for(const index of [1,3])expect(placed.compositions['new-timeline'].content.tracks[index].clips[1]).toMatchObject({placement:{timelineStartTick:5000000}})
})

it('copies complete transition windows without overlapping the original rendered span', () => {
  const input = JSON.parse(JSON.stringify(fixture.nativeFirstFree[0]!.after))
  input.compositions['new-timeline'].content.tracks[1].transitions = [
    { id: 'enter', kind: 'dissolve', toClipId: 'auto-first', durationTicks: 0, timeline: { startTick: String(1750000n*254016n), durationTick: String(500000n*254016n) } },
    { id: 'leave', kind: 'dissolve', fromClipId: 'auto-first', durationTicks: 0, timeline: { startTick: String(4750000n*254016n), durationTick: String(500000n*254016n) } },
  ]
  let serial = 0
  const output = duplicateNativeClips(input, 'new-timeline', ['auto-first'], () => `transition-copy-${serial++}`)
  expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[1]).toMatchObject({ placement: { timelineStartTick: 5500000 } })
  expect(output.compositions['new-timeline']!.content.tracks[1]).toMatchObject({ transitions: [{ id: 'enter' }, { id: 'leave' }, { toClipId: 'transition-copy-0', timeline: { startTick: String(5250000n*254016n) } }, { fromClipId: 'transition-copy-0', timeline: { startTick: String(8250000n*254016n) } }] })
  expect(output.compositions['new-timeline']!.durationUs).toBe(8750000)
})
it('requires transition partners and sampled control clips, and permits repeated control-layer copying', () => {
  const control = structuredClone(fixture.nativeRebinds[0]!.before) as unknown as NativeNestedProject
  let serial = 0
  expect(() => duplicateNativeClips(control, 'root', ['picture'], () => `bad-${serial++}`)).toThrow('дорожки')
  const first = duplicateNativeClips(control, 'root', ['picture','control-clip'], () => `layer-copy-${serial++}`)
  const ids = first.compositions.root!.content.tracks.flatMap(track => track.clips.map(clip => clip.id)).filter(id => id.startsWith('layer-copy'))
  expect(() => duplicateNativeClips(first, 'root', ids, () => `again-${serial++}`)).not.toThrow()
  const audio = JSON.parse(JSON.stringify(fixture.nativeDeleteCrossfade.before))
  const audioTrack = audio.compositions.root.content.tracks.find((track: {kind:string}) => track.kind==='audio')
  expect(() => duplicateNativeClips(audio, 'root', [audioTrack.clips[0].id], () => `audio-copy-${serial++}`)).toThrow('аудиоперехода')
})
