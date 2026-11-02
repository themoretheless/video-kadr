import {expect,it} from 'vitest'
import {patchWhiteBalanceSourceKey,setWhiteBalanceSourceKey,removeWhiteBalanceSourceKey,type WhiteBalanceEffect} from './concatWhiteBalanceSourceKeys'
it('preserves the other channel, duplicates and easing and enforces authored limits',()=>{
 const effect:WhiteBalanceEffect={kind:'concat_white_balance',parameters:{temperature:6500,tint:12},concatKeys:{temperature:[{at:0.5,value:6500},{at:0.5,value:7500,ease:[0.42,-2,0.58,3]}],tint:[{at:0,value:20}]}}
 const patched=patchWhiteBalanceSourceKey(effect,'temperature',1,{value:8500});expect(patched.parameters).toBe(effect.parameters);expect(patched.concatKeys?.tint).toBe(effect.concatKeys?.tint);expect(patched.concatKeys?.temperature?.[1]?.ease).toEqual([0.42,-2,0.58,3]);expect(effect.concatKeys?.temperature?.[1]?.value).toBe(7500)
 expect(setWhiteBalanceSourceKey(effect,'temperature',{at:0.501,value:9000}).concatKeys?.temperature).toHaveLength(2)
 expect(removeWhiteBalanceSourceKey(effect,'tint',0).concatKeys?.tint).toBeUndefined()
 expect(()=>setWhiteBalanceSourceKey(effect,'temperature',{at:0,value:1999})).toThrow();expect(()=>setWhiteBalanceSourceKey(effect,'tint',{at:0,value:101})).toThrow()
 expect(()=>setWhiteBalanceSourceKey({...effect,animation:{tint:{mode:'constant',value:0}}},'temperature',{at:0,value:6500})).toThrow()
 const full:WhiteBalanceEffect={...effect,concatKeys:{tint:Array.from({length:4096},()=>({at:0,value:0}))}};expect(()=>setWhiteBalanceSourceKey(full,'temperature',{at:1,value:6500})).toThrow()
})

import {createComposition} from './commands'
import {normalizeComposition} from './validation'
import {buildCompositionWireDocument,buildCompositionRenderRequest} from './payload'
import {rebaseNativeEffects} from './nativeEffects'
import type {WireVideoEffect} from './types'
it('preserves original white balance keys and easing through persistence, export and rebasing',()=>{
 const effect:WireVideoEffect={kind:'concat_white_balance',parameters:{temperature:6500,tint:12},concatKeys:{temperature:[{at:0,value:6500},{at:1,value:7500,ease:[0.42,3,0.58,3]}],tint:[{at:0,value:25}]}}
 const c={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'image' as const,durationTicks:0,width:320,height:180,hasAudio:false}},tracks:[{id:'image',kind:'image' as const,name:'Image',hidden:false,muted:false,locked:false,clips:[{id:'a',kind:'image' as const,sourceId:'source',timelineStartTicks:1000000,durationTicks:1000000,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain' as const},effects:[effect]}],transitions:[]}]}
 const restored=normalizeComposition(JSON.parse(JSON.stringify(c)));const wire=buildCompositionWireDocument(restored)
 expect(wire.tracks[0]).toMatchObject({clips:[{effects:[effect]}]});expect(buildCompositionRenderRequest(restored).composition).toEqual(wire)
 expect(rebaseNativeEffects([effect],-1250000)).toEqual([effect]);expect(effect.concatKeys?.temperature?.[1]?.ease).toEqual([0.42,3,0.58,3])
})
