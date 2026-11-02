import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'

test('production placement preserves exact transition ticks through native relocation',async({page,request})=>{
 const fixture=JSON.parse(await readFile(new URL('../../backend/tests/fixtures/concat-merge-preview.json',import.meta.url),'utf8'))
 const project=structuredClone(fixture.nativeFirstFree[0].after),scope='new-timeline',track=project.compositions[scope].content.tracks[1],unit=254016n
 const resident=structuredClone(track.clips[0]);resident.id='resident';resident.placement={...resident.placement,timelineStartTick:0,sourceInTick:1000000,sourceOutTick:2000000};track.clips.unshift(resident)
 const start=1500000n*unit-1n
 track.transitions=[{id:'enter',kind:'dissolve',toClipId:'auto-first',durationTicks:0,reverse:false,timeline:{startTick:String(start),durationTick:String(1000000n*unit)}}]
 const before=JSON.stringify(project)
 await page.goto('/')
 const moves=await page.evaluate(async({project,scope})=>{
  const path='/src/lib/composition/nativeClipMove.ts'
  const {nativeClipMoveBatch}=await import(/* @vite-ignore */ path)
  return nativeClipMoveBatch(project,scope,scope,['auto-first'],1000000,{},()=> 'unused')
 },{project,scope})
 expect(moves.map(move=>move.startUs)).toEqual([1500001,1500001])
 const response=await request.post('/api/compositions/nested-move-clips',{data:{project,moves}});expect(response.status(),await response.text()).toBe(200)
 const result=await response.json(),clips=result.project.compositions[scope].content.tracks[1].clips
 expect(result.changed).toBe(true);expect(clips.find((clip:{id:string})=>clip.id==='resident').placement.timelineStartTick).toBe(0);expect(clips.find((clip:{id:string})=>clip.id==='auto-first').placement.timelineStartTick).toBe(1500001)
 expect(result.project.compositions[scope].content.tracks[1].transitions[0].timeline.startTick).toBe(String(start-499999n*unit))
 const duplicated=await page.evaluate(async({project,scope})=>{
  const path='/src/lib/composition/nativeClipDuplicate.ts'
  const {duplicateNativeClips}=await import(/* @vite-ignore */ path);let serial=0
  return duplicateNativeClips(project,scope,['auto-first'],()=>`copy-${serial++}`,1000000)
 },{project,scope})
 expect(duplicated.compositions[scope].content.tracks[1].clips.find((clip:{id:string})=>clip.id==='copy-0').placement.timelineStartTick).toBe(5500001)
 expect(duplicated.compositions[scope].content.tracks[3].clips.find((clip:{id:string})=>clip.id==='copy-1').placement.timelineStartTick).toBe(5500001)
 expect(duplicated.compositions[scope].content.tracks[1].transitions.find((transition:{id:string})=>transition.id==='copy-2').timeline.startTick).toBe(String(start+3500001n*unit))
 const admitted=await request.post('/api/compositions/nested-frame',{data:{project:duplicated,timeUs:0}});expect(admitted.status(),await admitted.text()).toBe(200)
 expect(JSON.stringify(project)).toBe(before)
})
