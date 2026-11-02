import {expect,it} from 'vitest'
import oracle from '../../../../backend/tests/fixtures/filmcraft-caption-insert.json'
import {insertCaptionGap} from './captionInsert'
import {exactCaptionTicks} from './captionTime'
import type {CaptionTrack} from './types'
it('matches all original global insert caption boundaries, split IDs and metadata',()=>{
 for(const scenario of oracle.cases){
  const tracks:CaptionTrack[]=scenario.before.map(track=>({id:`track-${track.id}`,name:track.name,format:'Subtitle',language:track.language,locked:track.locked,syncLock:track.sync_lock,hidden:!track.enabled,vttBlocks:['NOTE retained'],clips:track.captions.map(c=>({id:`cue-${c.id}`,cueId:c.cue_id,text:c.text,speaker:c.speaker,settings:c.settings,timelineStartTick:String(c.start),timelineEndTick:String(c.start+c.duration)}))}))
  const before=JSON.stringify(tracks);let nextId=2000
  const result=insertCaptionGap(tracks,scenario.at,scenario.duration,()=>`cue-${nextId++}`)
  expect(result.map(track=>track.clips.map(c=>({id:c.id,cueId:c.cueId??null,text:c.text,speaker:c.speaker,settings:c.settings,start:exactCaptionTicks(c.timelineStartTick),end:exactCaptionTicks(c.timelineEndTick)})))).toEqual(scenario.after.map(track=>track.captions.map(c=>({id:`cue-${c.id}`,cueId:c.cue_id,text:c.text,speaker:c.speaker,settings:c.settings,start:BigInt(c.start),end:BigInt(c.start+c.duration)}))))
  expect(result.map(track=>track.vttBlocks)).toEqual(tracks.map(track=>track.vttBlocks))
  expect(JSON.stringify(tracks)).toBe(before)
  if(scenario.name==='locked'||scenario.name==='sync-off'){expect(nextId).toBe(2000);result.forEach((track,index)=>expect(track).toBe(tracks[index]))}
 }
})
it('keeps legacy integer-us encoding and refuses colliding IDs and overflow atomically',()=>{
 const tracks:CaptionTrack[]=[{id:'track',name:'Captions',clips:[{id:'cue',cueId:'file',timelineStartTick:0,timelineEndTick:2000000,text:'Text'}]}]
 const result=insertCaptionGap(tracks,1000000,500000,()=> 'right')
 expect(result[0].clips).toMatchObject([{id:'cue',cueId:'file',timelineEndTick:1000000},{id:'right',cueId:null,timelineStartTick:1500000,timelineEndTick:2500000}])
 const before=JSON.stringify(tracks)
 expect(()=>insertCaptionGap(tracks,1000000,500000,()=> 'cue')).toThrow()
 expect(()=>insertCaptionGap(tracks,1000000,500000,()=> 'ordinary',new Set(['ordinary']))).toThrow()
 expect(()=>insertCaptionGap(tracks,0,-1,()=> 'right')).toThrow()
 expect(JSON.stringify(tracks)).toBe(before)
 const exact:CaptionTrack[]=[{id:'track',name:'Exact',clips:[{id:'cue',timelineStartTick:'9223372036854775806',timelineEndTick:'9223372036854775807',text:'Exact'}]}]
 expect(()=>insertCaptionGap(exact,'0','1',()=> 'right')).toThrow()
 expect(exact[0].clips[0].timelineEndTick).toBe('9223372036854775807')
})
