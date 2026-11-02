import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { expandNativeClipLinks, linkNativeClips, unlinkNativeClips } from './nativeClipLinks'
import { editNativeTrack } from './nativeTrackEdit'
const project = () => structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as NativeNestedProject
it('unlinks the entire selected group without changing authored clips and can relink it', () => {
  const input = project(), before = JSON.stringify(input)
  const output = unlinkNativeClips(input, 'new-timeline', ['auto-first'])
  expect(output.compositions['new-timeline']!.content.metadata!.clipLinks).toEqual({})
  expect(output.compositions['new-timeline']!.content.tracks).toBe(input.compositions['new-timeline']!.content.tracks)
  expect(output.compositions.root).toBe(input.compositions.root)
  expect(unlinkNativeClips(output, 'new-timeline', ['auto-first'])).toBe(output)
  const linked = linkNativeClips(output, 'new-timeline', ['auto-first', 'auto-first-audio'])
  expect(expandNativeClipLinks(linked, 'new-timeline', ['auto-first'])).toEqual(['auto-first', 'auto-first-audio'])
  expect(JSON.stringify(input)).toBe(before)
})
it('refuses missing members, existing links and locked companions atomically', () => {
  const input = project()
  expect(() => unlinkNativeClips(input, 'new-timeline', ['missing'])).toThrow('не найден')
  expect(() => linkNativeClips(input, 'new-timeline', ['auto-first', 'auto-first-audio'])).toThrow('уже связан')
  const locked = editNativeTrack(input, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  const before = JSON.stringify(locked)
  expect(() => unlinkNativeClips(locked, 'new-timeline', ['auto-first'])).toThrow('заблокирована')
  const unlinked = unlinkNativeClips(input, 'new-timeline', ['auto-first'])
  const lockedUnlinked = editNativeTrack(unlinked, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  expect(() => linkNativeClips(lockedUnlinked, 'new-timeline', ['auto-first', 'auto-first-audio'])).toThrow('заблокирована')
  expect(JSON.stringify(locked)).toBe(before)
})
