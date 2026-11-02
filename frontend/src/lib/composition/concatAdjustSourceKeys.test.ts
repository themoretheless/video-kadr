import {expect,it} from 'vitest'
import {createComposition} from './commands'
import {normalizeComposition} from './validation'
import {buildCompositionWireDocument,buildCompositionRenderRequest} from './payload'
import {rebaseNativeEffects} from './nativeEffects'
import type {WireVideoEffect} from './types'
it('preserves original adjust wheels keys and easing through persistence, export and rebasing',()=>{
 const effect:WireVideoEffect={kind:'concat_adjust',parameters:{lift:[0.1,0,0]},concatKeys:{'lift.x':[{at:0,value:0},{at:1,value:0.2,ease:[0.42,3,0.58,3]}],'gain.m':[{at:0,value:0.1}]}}
 const c={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'image' as const,durationTicks:0,width:320,height:180,hasAudio:false}},tracks:[{id:'image',kind:'image' as const,name:'Image',hidden:false,muted:false,locked:false,clips:[{id:'a',kind:'image' as const,sourceId:'source',timelineStartTicks:1000000,durationTicks:1000000,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain' as const},effects:[effect]}],transitions:[]}]}
 const restored=normalizeComposition(JSON.parse(JSON.stringify(c)));const wire=buildCompositionWireDocument(restored)
 expect(wire.tracks[0]).toMatchObject({clips:[{effects:[effect]}]});expect(buildCompositionRenderRequest(restored).composition).toEqual(wire)
 expect(rebaseNativeEffects([effect],-1250000)).toEqual([effect]);expect(effect.concatKeys?.['lift.x']?.[1]?.ease).toEqual([0.42,3,0.58,3])
})
