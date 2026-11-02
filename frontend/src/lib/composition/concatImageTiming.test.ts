import {expect,it} from 'vitest'
import {createComposition} from './commands'
import {normalizeComposition,validateComposition} from './validation'
import {buildCompositionWireDocument} from './payload'
function fixture(){return {...createComposition({width:16,height:16,fps:24,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'image' as const,durationTicks:0,width:16,height:16,hasAudio:false}},tracks:[{id:'image',kind:'image' as const,name:'Image',hidden:false,muted:false,locked:false,clips:[{id:'clip',kind:'image' as const,sourceId:'source',timelineStartTicks:1000000,durationTicks:2000000,opacity:1,transform:{x:0,y:0,width:16,height:16,fit:'contain' as const},concatSourceTiming:{sourceStart:3.123456789,speed:2}}]}]}}
it('retains source In and speed through normalization and the native payload without changing placement',()=>{
 const composition=normalizeComposition(JSON.parse(JSON.stringify(fixture())))
 expect(validateComposition(composition)).toEqual([])
 const wire=buildCompositionWireDocument(composition)
 expect(wire.tracks[0]).toMatchObject({clips:[{timelineStartTick:1000000,durationTicks:2000000,concatSourceTiming:{sourceStart:3.123456789,speed:2}}]})
})
it('refuses invalid image source metadata',()=>{
 for(const timing of [{sourceStart:-1,speed:2},{sourceStart:0,speed:0},{sourceStart:0,speed:17},{sourceStart:0,speed:1,future:true}]){
  const value=fixture();Object.assign(value.tracks[0].clips[0],{concatSourceTiming:timing});expect(validateComposition(value).some(issue=>issue.code==='image-source-timing')).toBe(true)
 }
})
