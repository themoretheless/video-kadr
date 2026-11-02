import {expect,it} from 'vitest'
import {createComposition} from './commands'
import {normalizeComposition} from './validation'
import {buildCompositionWireDocument,buildCompositionRenderRequest} from './payload'
import {rebaseNativeEffects} from './nativeEffects'
import type {WireVideoEffect} from './types'
it('preserves original saturation keys and easing through persistence, export and rebasing',()=>{
 const effect:WireVideoEffect={kind:'concat_saturation',parameters:{amount:-1},concatKeys:[{at:0,value:0},{at:1,value:2,ease:[0.42,3,0.58,3]}]}
 const c={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'image' as const,durationTicks:0,width:320,height:180,hasAudio:false}},tracks:[{id:'image',kind:'image' as const,name:'Image',hidden:false,muted:false,locked:false,clips:[{id:'a',kind:'image' as const,sourceId:'source',timelineStartTicks:1000000,durationTicks:1000000,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain' as const},effects:[effect]}],transitions:[]}]}
 const restored=normalizeComposition(JSON.parse(JSON.stringify(c)));const wire=buildCompositionWireDocument(restored)
 expect(wire.tracks[0]).toMatchObject({clips:[{effects:[effect]}]});expect(buildCompositionRenderRequest(restored).composition).toEqual(wire)
 expect(rebaseNativeEffects([effect],-1250000)).toEqual([effect]);expect(effect.concatKeys?.[1]?.ease).toEqual([0.42,3,0.58,3])
})

import {patchSourceSaturationKey,setSourceSaturationKey,removeSourceSaturationKey} from './concatSaturationSourceKeyEditing'
it('edits source keys without losing duplicates, easing or base settings',()=>{
 const effect={kind:'concat_saturation' as const,parameters:{amount:-1},concatKeys:[{at:0.5,value:0},{at:0.501,value:1},{at:0.501,value:2,ease:[0.42,-2,0.58,3] as const}]}
 const patched=patchSourceSaturationKey(effect,2,{value:3.5});expect(patched.parameters).toBe(effect.parameters);expect(patched.concatKeys?.[2]?.ease).toEqual([0.42,-2,0.58,3]);expect(effect.concatKeys[2]?.value).toBe(2)
 expect(setSourceSaturationKey(effect,{at:0.5011,value:4}).concatKeys).toEqual([{at:0.5,value:0},{at:0.501,value:2,ease:[0.42,-2,0.58,3]},{at:0.5011,value:4}])
 expect(removeSourceSaturationKey({kind:'concat_saturation',parameters:{},concatKeys:[{at:0,value:1}]},0).concatKeys).toBeUndefined()
 expect(()=>setSourceSaturationKey(effect,{at:0,value:100.01})).toThrow();expect(()=>setSourceSaturationKey({...effect,animation:{mode:'constant',value:0}},{at:0,value:1})).toThrow()
 const full={kind:'concat_saturation' as const,parameters:{},concatKeys:Array.from({length:4096},()=>({at:0,value:0}))};expect(()=>setSourceSaturationKey(full,{at:1,value:1})).toThrow()
})
