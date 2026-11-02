import { removeNativeClips } from './nativeClipDelete'
import { expect,it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { captureNativeClipboard,prepareNativeClipboardPaste } from './nativeClipboard'
import { pasteNativeCaptionTracks } from './nativeCaptionClipboard'
import { exactCaptionTicks } from './captionTime'
function source() {
  const project=structuredClone(fixture.nativeCrossCopies[0]!.before) as unknown as NativeNestedProject
  const node=project.compositions['new-timeline']!
  project.compositions['new-timeline']={...node,content:{...node.content,captionTracks:[{id:'captions',name:'Russian',format:'Subtitle',language:'ru',syncLock:false,hidden:false,locked:false,style:{font:'Inter',size:8,background:false},vttBlocks:['NOTE retained'],clips:[{id:'cue',cueId:'import-id',timelineStartTick:String(1000000n*254016n+1n),timelineEndTick:String(3000000n*254016n+3n),text:'<b>Текст</b>',speaker:'Автор',settings:'align:start'}]}]}}
  return project
}
it('copies mixed media and exact captions with a common delta, retaining style, markup and source independence',()=>{
  const project=source();let serial=0
  const clipboard=captureNativeClipboard(project,'new-timeline',['auto-first','cue'],()=>`capture-${serial++}`)
  const saved=JSON.stringify(clipboard)
  const plan=prepareNativeClipboardPaste(project,clipboard,'copy-destination',8000000,{},()=>`paste-${serial++}`)
  expect(plan.moves.every(move=>move.startUs===9000000)).toBe(true)
  expect(plan.captionDeltaUs).toBe(7000000)
  const result=pasteNativeCaptionTracks(plan.base,'copy-destination',plan.captions,plan.captionDeltaUs,{},()=>`caption-${serial++}`)
  const track=result.compositions['copy-destination']!.content.captionTracks![0]!, cue=track.clips[0]!
  expect(exactCaptionTicks(cue.timelineStartTick)).toBe(8000000n*254016n+1n)
  expect(exactCaptionTicks(cue.timelineEndTick)).toBe(10000000n*254016n+3n)
  expect(cue).toMatchObject({cueId:'import-id',text:'<b>Текст</b>',speaker:'Автор',settings:'align:start'})
  expect(track.style).toEqual({font:'Inter',size:8,background:false})
  expect(track.vttBlocks).toEqual(['NOTE retained']);expect(JSON.stringify(clipboard)).toBe(saved)
  expect(result.compositions['new-timeline']).toBe(project.compositions['new-timeline'])
})
it('pastes caption-only content without a media transfer and refuses locks, overlaps and mismatched styles atomically',()=>{
  const project=source();let serial=0
  const clipboard=captureNativeClipboard(project,'new-timeline',['cue'],()=>`capture-${serial++}`)
  const plan=prepareNativeClipboardPaste(project,clipboard,'new-timeline',1000000,{captions:'captions'},()=>`paste-${serial++}`)
  expect(plan.moves).toEqual([])
  const before=JSON.stringify(project)
  expect(()=>pasteNativeCaptionTracks(project,'new-timeline',plan.captions,0,{captions:'captions'},()=>`overlap-${serial++}`)).toThrow('перекрываются')
  const node=project.compositions['new-timeline']!, track=node.content.captionTracks![0]!
  const locked={...project,compositions:{...project.compositions,'new-timeline':{...node,content:{...node.content,captionTracks:[{...track,locked:true}]}}}}
  expect(()=>pasteNativeCaptionTracks(locked,'new-timeline',plan.captions,8000000,{captions:'captions'},()=>`locked-${serial++}`)).toThrow('заблокирована')
  expect(()=>captureNativeClipboard(locked,'new-timeline',['cue'],()=>`locked-${serial++}`)).toThrow('заблокирована')
  const different={...project,compositions:{...project.compositions,'new-timeline':{...node,content:{...node.content,captionTracks:[{...track,style:{size:20}}]}}}}
  expect(()=>pasteNativeCaptionTracks(different,'new-timeline',plan.captions,8000000,{captions:'captions'},()=>`style-${serial++}`)).toThrow('Оформление')
  const appended=pasteNativeCaptionTracks(project,'new-timeline',plan.captions,7000000,{captions:'captions'},()=>`accepted-${serial++}`)
  expect(appended.compositions['new-timeline']!.content.captionTracks).toHaveLength(1)
  expect(appended.compositions['new-timeline']!.content.captionTracks![0]!.clips).toHaveLength(2)
  expect(exactCaptionTicks(appended.compositions['new-timeline']!.content.captionTracks![0]!.clips[1]!.timelineStartTick)).toBe(8000000n*254016n+1n)
  expect(JSON.stringify(project)).toBe(before)
})

it('removes mixed linked media/caption selections atomically and retains caption track styling',()=>{
  const input=source(),node=input.compositions['new-timeline']!,track=node.content.captionTracks![0]!
  const removed=removeNativeClips(input,'new-timeline',['auto-first','cue'])
  expect(removed.compositions['new-timeline']!.content.tracks.reduce((count,track)=>count+track.clips.length,0)).toBe(0)
  expect(removed.compositions['new-timeline']!.content.captionTracks).toEqual([{...track,clips:[]}])
  const locked={...input,compositions:{...input.compositions,'new-timeline':{...node,content:{...node.content,captionTracks:[{...track,locked:true}]}}}}
  const before=JSON.stringify(locked)
  expect(()=>removeNativeClips(locked,'new-timeline',['auto-first','cue'])).toThrow('заблокирована')
  expect(JSON.stringify(locked)).toBe(before)
})
