import {expect,it} from 'vitest'
import oracle from '../../../../backend/tests/fixtures/filmcraft-caption-file-import.json'
import {applyPreparedCaptionImport} from './captionFileImport'
import {createComposition} from './commands'
import type {PreparedCaptionFileImport} from '$lib/api'
it('preserves original imported track metadata and exact placements for all fourteen original transactions',()=>{
 expect(oracle.commit).toBe('5231852443363f001c3f6b396dd9b1e6461ae2be')
 for(const scenario of oracle.cases){
  const original=scenario.after[0]
  const prepared:PreparedCaptionFileImport={name:original.name,format:(original.format==='Cea608'?'CEA-608':original.format==='Cea708'?'CEA-708':original.format) as PreparedCaptionFileImport['format'],language:original.language,enabled:original.enabled,locked:original.locked,syncLock:original.sync_lock,backgroundColor:original.style.background_color as [number,number,number,number],document:{ticksPerSecond:'254016000000',blocks:original.vtt_blocks,cues:original.captions.map(cue=>({startTick:String(cue.start),endTick:String(cue.start+cue.duration),text:cue.text,id:cue.cue_id,speaker:cue.speaker,settings:cue.settings}))}}
  const before={...createComposition({width:320,height:180,fps:scenario.frameRate.num/scenario.frameRate.den,fpsRate:{numerator:scenario.frameRate.num,denominator:scenario.frameRate.den},backgroundColor:'#000000'}),captionTracks:[{id:'keep',name:'Keep',locked:true,clips:[]}]}
  const snapshot=JSON.stringify(before);let id=0
  const result=applyPreparedCaptionImport(before,prepared,()=>String(++id)),track=result.composition.captionTracks![0]
  expect(track).toMatchObject({name:original.name,format:prepared.format,language:original.language,hidden:!original.enabled,locked:original.locked,syncLock:original.sync_lock,vttBlocks:original.vtt_blocks})
  expect(track.clips.map(cue=>({start:BigInt(cue.timelineStartTick),duration:BigInt(cue.timelineEndTick)-BigInt(cue.timelineStartTick),text:cue.text,cueId:cue.cueId,speaker:cue.speaker,settings:cue.settings}))).toEqual(original.captions.map(cue=>({start:BigInt(cue.start),duration:BigInt(cue.duration),text:cue.text,cueId:cue.cue_id,speaker:cue.speaker,settings:cue.settings})))
  expect(track.id).toBe(result.trackId);expect(result.composition.captionTracks![1]).toEqual(before.captionTracks[0]);expect(JSON.stringify(before)).toBe(snapshot)
 }
})
