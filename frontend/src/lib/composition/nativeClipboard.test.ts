import { expect,it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { captureNativeClipboard,prepareNativeClipboardPaste } from './nativeClipboard'
import { removeNativeClips } from './nativeClipDelete'
it('captures an independent snapshot and pastes after deleting the originals with fresh IDs on every paste',()=>{
  const input=structuredClone(fixture.nativeCrossCopies[13]!.before) as unknown as NativeNestedProject
  let serial=0
  const clipboard=captureNativeClipboard(input,'new-timeline',['auto-first'],()=>`capture-${serial++}`)
  const saved=JSON.stringify(clipboard)
  const deleted=removeNativeClips(input,'new-timeline',['auto-first'])
  const first=prepareNativeClipboardPaste(deleted,clipboard,'new-timeline',0,{},()=>`first-${serial++}`)
  const second=prepareNativeClipboardPaste(deleted,clipboard,'copy-destination',8000000,{},()=>`second-${serial++}`)
  expect(first.copies).toHaveLength(2);expect(second.copies).toHaveLength(2)
  expect(first.copies.some(id=>second.copies.includes(id))).toBe(false)
  expect(first.moves.every(move=>move.startUs===0)).toBe(true)
  expect(second.moves.every(move=>move.startUs===8000000)).toBe(true)
  expect(clipboard.compositions.clipboard!.content.metadata!.mediaVersions).toBeDefined()
  expect(JSON.stringify(clipboard)).toBe(saved)
  expect(first.base).toEqual(deleted)
})
it('imports referenced LUTs and rebinds conflicting tables without modifying the destination look',()=>{
  const input=structuredClone(fixture.nativeCrossCopies[0]!.before) as unknown as NativeNestedProject
  const lut={id:'table',name:'Original',lut:{format:'cube' as const,intensity:100,text:'LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1'}}
  const node=input.compositions['new-timeline']!
  input.compositions['new-timeline']={...node,content:{...node.content,tracks:node.content.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>({...clip,effects:[{kind:'lumetri' as const,parameters:{inputLutLibrary:'table'}}]}))}:track)}}
  input.luts=[lut];let serial=0
  const clipboard=captureNativeClipboard(input,'new-timeline',['auto-first'],()=>`capture-${serial++}`)
  const destination={...input,luts:[]}
  const plan=prepareNativeClipboardPaste(destination,clipboard,'copy-destination',0,{},()=>`paste-${serial++}`)
  expect(plan.base.luts).toEqual([lut]);expect(destination.luts).toEqual([])
  const conflicting={...input,luts:[{...lut,lut:{...lut.lut,text:lut.lut.text.replace('1 1 1','0 0 0')}}]}
  const rebound=prepareNativeClipboardPaste(conflicting,clipboard,'copy-destination',0,{},()=>`rebound-${serial++}`)
  expect(rebound.base.luts).toHaveLength(2)
  expect(rebound.base.luts![0]).toEqual(conflicting.luts[0])
  const added=rebound.base.luts![1]!
  expect(added.id).not.toBe('table');expect(added.lut).toEqual(lut.lut)
  const picture=rebound.staged.compositions[rebound.temporary]!.content.tracks.flatMap(track=>track.kind==='video'?track.clips:[])[0]!
  expect(picture.effects).toEqual([{kind:'lumetri',parameters:{inputLutLibrary:added.id}}])
})
