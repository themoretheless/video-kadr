import { expect,it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { parseNativeClipboard,serializeNativeClipboard } from './nativeClipboardText'
it('round-trips all qualified media, caption, exact-clock, external-controller and LUT clipboard snapshots',()=>{
  for (const test of [...fixture.nativeClipboardPastes,...fixture.nativeCaptionPastes]) {
    const original=test.clipboard as unknown as NativeNestedProject
    expect(parseNativeClipboard(serializeNativeClipboard(original))).toEqual(original)
  }
})
it('refuses unrelated text, unsupported envelopes, empty clips and nested-project imports',()=>{
  const project=structuredClone(fixture.nativeCaptionPastes[0]!.clipboard) as unknown as NativeNestedProject
  expect(()=>parseNativeClipboard('not a clip selection')).toThrow('Video Kadr')
  const value=JSON.parse(serializeNativeClipboard(project))
  for (const envelope of [{...value,version:2},{...value,format:'other'},{...value,extra:true}]) expect(()=>parseNativeClipboard(JSON.stringify(envelope))).toThrow('не поддерживается')
  const node=project.compositions.clipboard!
  expect(()=>serializeNativeClipboard({...project,compositions:{clipboard:{...node,instances:[{} as never]}}})).toThrow('Некорректный')
  expect(()=>serializeNativeClipboard({...project,compositions:{clipboard:node,other:node}})).toThrow('Некорректный')
  expect(()=>serializeNativeClipboard({...project,compositions:{clipboard:{...node,content:{...node.content,tracks:[],captionTracks:[]}}}})).toThrow('от 1 до 512')
})
