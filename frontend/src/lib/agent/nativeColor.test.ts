import {EASING_PRESETS} from '../composition/easingPresets'
import {packColorAnimation} from '../composition/colorAnimation'
import {expect,it} from 'vitest'
import {applyNativeColorOperation,readNativeColorTrack} from './nativeColor'
import {createComposition,addTrack,addClip} from '../composition/commands'
import {buildCompositionWireDocument} from '../composition/payload'
import type {NativeNestedProject,NativeColorTrack} from '../api'
function project():NativeNestedProject{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000FF'})
 doc=addTrack(doc,{id:'titles',kind:'text',name:'Titles',locked:false,hidden:false,clips:[]})
 doc=addClip(doc,'titles',{id:'title',kind:'text',timelineStartTicks:1000000,durationTicks:3000000,text:'Color',x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#12345680',align:'center'}})
 return {schemaVersion:1,root:'root',compositions:{root:{content:{...buildCompositionWireDocument(doc),tracks:buildCompositionWireDocument(doc).tracks.map(track=>({...track,locked:true}))},instances:[]}}}
}
it('reads static RGBA bytes on locked native text without modifying the project',()=>{
 const input=project(),before=JSON.stringify(input)
 expect(readNativeColorTrack(input,'root',{clipId:'title'})).toEqual({mode:'constant',value:{r:18,g:52,b:86,a:128}})
 expect(JSON.stringify(input)).toBe(before)
})
it('retains signed fractional native key times, controls, legacy curves and detached packed data',()=>{
 const input=project(),clip=input.compositions.root!.content.tracks[0]!.clips[0]!
 const animation:NativeColorTrack={keys:[{t_us:-0.5,value:{r:18,g:52,b:86,a:128},in_:[0.6,0.2],out:[0.3,0.8],segment:{kind:'Linear'},legacy_curve:'ease_in_out'}],extrapolate:{before:'Hold',after:'Hold'}}
 Object.assign(clip,{colorAnimation:animation})
 const result=readNativeColorTrack(input,'root',{clipId:'title'})
 expect(result).toMatchObject({mode:'keyframes',timeBase:1000000,nativeTrack:animation,keyframes:[{tick:-0.5,t_local_us:-0.5,t_us:999999.5,incoming:{x:0.6,y:0.2},legacy_curve:'ease_in_out'}]})
 if(result.mode==='keyframes')expect(result.nativeTrack).not.toBe(animation)
 expect(()=>readNativeColorTrack(input,'missing',{clipId:'title'})).toThrow('not found')
 expect(()=>readNativeColorTrack(input,'root',{clipId:'title',unknown:true})).toThrow('Invalid')
})

it('reads native byte-valued color layers without scaling bytes a second time',()=>{
 const input=project(),source=input.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in source))throw new Error('Visual fixture required')
 const color={r:12,g:34,b:56,a:78}
 const projectWithColor:NativeNestedProject={...input,compositions:{root:{instances:[],content:{...input.compositions.root!.content,tracks:[{id:'colors',kind:'color',name:'Colors',hidden:false,locked:false,clips:[{id:'fill',timelineStartTick:0,timelineEndTick:1000000,width:16,height:16,color,transform:source.transform,opacity:{mode:'constant',value:1},enabled:true}]}]}}}}
 expect(readNativeColorTrack(projectWithColor,'root',{clipId:'fill'})).toEqual({mode:'constant',value:color})
})

it('adds anchored color keys, preserves packed controls and deletes the last key to its color',()=>{
 const input=project(),node=input.compositions.root!
 const unlocked={...input,compositions:{root:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,locked:false}))}}}}
 const before=JSON.stringify(unlocked)
 const output=applyNativeColorOperation(unlocked,'root',{op:'set_color_key',clipId:'title',playheadTicks:1500000,value:{r:255,g:0,b:0,a:128}})
 expect(readNativeColorTrack(output,'root',{clipId:'title'})).toMatchObject({mode:'keyframes',keyframes:[{tick:0,value:{r:18,g:52,b:86,a:128}},{tick:500000,value:{r:255,g:0,b:0,a:128}}]})
 const one=applyNativeColorOperation(output,'root',{op:'delete_color_key',clipId:'title',tick:0})
 const constant=applyNativeColorOperation(one,'root',{op:'delete_color_key',clipId:'title',tick:500000})
 expect(readNativeColorTrack(constant,'root',{clipId:'title'})).toEqual({mode:'constant',value:{r:255,g:0,b:0,a:128}})
 expect(JSON.stringify(unlocked)).toBe(before)
 expect(()=>applyNativeColorOperation(input,'root',{op:'set_color_key',clipId:'title',playheadTicks:1000000,value:{r:1,g:2,b:3,a:4}})).toThrow('locked')
 expect(()=>applyNativeColorOperation(unlocked,'root',{op:'set_color_key',clipId:'title',playheadTicks:999999,value:{r:1,g:2,b:3,a:4}})).toThrow('outside')
})

it('edits packed keys without changing neighboring fractional timing or legacy controls',()=>{
 const input=project(),node=input.compositions.root!
 const unlocked={...input,compositions:{root:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,locked:false}))}}}}
 const clip=unlocked.compositions.root.content.tracks[0]!.clips[0]!
 const key={t_us:-0.5,value:{r:1,g:2,b:3,a:4},in_:[0.6,0.9] as const,out:[0.2,0.7] as const,segment:{kind:'Linear' as const},legacy_curve:'ease_in_out' as const}
 Object.assign(clip,{colorAnimation:{keys:[key],extrapolate:{before:'Hold',after:'Hold'}}})
 const updated=applyNativeColorOperation(unlocked,'root',{op:'set_color_key',clipId:'title',playheadTicks:1500000,value:{r:9,g:8,b:7,a:6}})
 const result=readNativeColorTrack(updated,'root',{clipId:'title'})
 if(result.mode!=='keyframes')throw new Error('Key track required')
 expect(result.nativeTrack.keys[0]).toEqual(key)
 const removed=applyNativeColorOperation(updated,'root',{op:'delete_color_key',clipId:'title',tick:-0.5})
 expect(readNativeColorTrack(removed,'root',{clipId:'title'})).toMatchObject({keyframes:[{tick:500000}]})
})

it('moves and recolors exact fractional native keys while retaining temporal controls',()=>{
 const input=project(),node=input.compositions.root!
 const unlocked={...input,compositions:{root:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,locked:false}))}}}}
 const clip=unlocked.compositions.root.content.tracks[0]!.clips[0]!
 const key={t_us:-0.5,value:{r:1,g:2,b:3,a:4},in_:[0.6,0.9] as const,out:[0.2,0.7] as const,segment:{kind:'Linear' as const},legacy_curve:'ease_in_out' as const}
 Object.assign(clip,{colorAnimation:{keys:[key,{...key,t_us:10}],extrapolate:{before:'Hold',after:'Hold'}}})
 const before=JSON.stringify(unlocked)
 const edited=applyNativeColorOperation(unlocked,'root',{op:'edit_color_key',clipId:'title',tick:-0.5,newTick:-1.25,value:{r:9,g:8,b:7,a:6}})
 const result=readNativeColorTrack(edited,'root',{clipId:'title'})
 if(result.mode!=='keyframes')throw new Error('Key track required')
 expect(result.nativeTrack.keys[0]).toEqual({...key,t_us:-1.25,value:{r:9,g:8,b:7,a:6}})
 expect(result.nativeTrack.keys[1]).toEqual({...key,t_us:10})
 for(const patch of [{newTick:10},{newTick:Infinity},{value:{r:256,g:0,b:0,a:255}},{}])expect(()=>applyNativeColorOperation(unlocked,'root',{op:'edit_color_key',clipId:'title',tick:-0.5,...patch})).toThrow()
 expect(JSON.stringify(unlocked)).toBe(before)
})

it('sets every native extrapolation mode independently without changing packed keys',()=>{
 const input=project(),node=input.compositions.root!
 const unlocked={...input,compositions:{root:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,locked:false}))}}}}
 const keyed=applyNativeColorOperation(unlocked,'root',{op:'set_color_key',clipId:'title',playheadTicks:1500000,value:{r:1,g:2,b:3,a:4}})
 const before=readNativeColorTrack(keyed,'root',{clipId:'title'})
 if(before.mode!=='keyframes')throw new Error('Keys required')
 const modes={hold:'Hold',continue:'Continue',loop:'Loop',ping_pong:'PingPong',offset:'Offset'}
 for(const [extrapolation,expected] of Object.entries(modes))for(const side of ['before','after']){
  const updated=applyNativeColorOperation(keyed,'root',{op:'set_color_extrapolation',clipId:'title',side,extrapolation})
  const result=readNativeColorTrack(updated,'root',{clipId:'title'})
  if(result.mode!=='keyframes')throw new Error('Keys required')
  expect(result.nativeTrack.keys).toEqual(before.nativeTrack.keys)
  expect(result.extrapolation).toEqual({before:'Hold',after:'Hold',[side]:expected})
 }
 for(const patch of [{side:'wrong',extrapolation:'hold'},{side:'after',extrapolation:'toString'},{side:'before',extrapolation:'bad'}])expect(()=>applyNativeColorOperation(keyed,'root',{op:'set_color_extrapolation',clipId:'title',...patch})).toThrow('Invalid')
 expect(()=>applyNativeColorOperation(unlocked,'root',{op:'set_color_extrapolation',clipId:'title',side:'after',extrapolation:'loop'})).toThrow('keys')
})

it('packs all native color presets with the established color converter while preserving clocks and values',()=>{
 const input=project(),node=input.compositions.root!
 const unlocked={...input,compositions:{root:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,locked:false}))}}}}
 const keyed=applyNativeColorOperation(unlocked,'root',{op:'set_color_key',clipId:'title',playheadTicks:1500000,value:{r:1,g:2,b:3,a:4}})
 for(const preset of EASING_PRESETS){
  const edited=applyNativeColorOperation(keyed,'root',{op:'set_color_preset',clipId:'title',tick:0,preset:preset.id})
  const result=readNativeColorTrack(edited,'root',{clipId:'title'})
  const expected=packColorAnimation({mode:'keyframes',track:{timeBase:1000000,interpolation:preset.interpolation,keyframes:[{tick:0,value:'#12345680'},{tick:500000,value:'#01020304'}]}})
  expect(result).toMatchObject({nativeTrack:expected})
 }
 for(const interpolation of [null,['linear'],{kind:'bezier',p1:[2,0],p2:[1,1]},'wrong'])expect(()=>applyNativeColorOperation(keyed,'root',{op:'set_color_interpolation',clipId:'title',interpolation})).toThrow('Invalid')
})

it('retains tangent authoring modes, solves Auto and permits independent Smooth color handles',()=>{
 const input=project(),node=input.compositions.root!
 const unlocked={...input,compositions:{root:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,locked:false}))}}}}
 const keyed=applyNativeColorOperation(unlocked,'root',{op:'set_color_key',clipId:'title',playheadTicks:1500000,value:{r:1,g:2,b:3,a:4}})
 const auto=applyNativeColorOperation(keyed,'root',{op:'set_color_tangent_mode',clipId:'title',tick:0,mode:'auto'})
 expect(readNativeColorTrack(auto,'root',{clipId:'title'})).toMatchObject({keyframes:[{outgoing:{x:1/3,y:1/3,mode:'auto'},continuity:'smooth',segment:{kind:'Spline'}},{}]})
 const edited=applyNativeColorOperation(auto,'root',{op:'edit_color_tangents',clipId:'title',tick:0,side:'outgoing',point:{x:0.4,y:1.7},continuity:'smooth'})
 expect(readNativeColorTrack(edited,'root',{clipId:'title'})).toMatchObject({keyframes:[{incoming:{x:2/3,y:2/3,mode:'free'},outgoing:{x:0.4,y:1.7,mode:'free'},continuity:'smooth'},{}]})
 const roundTrip=JSON.parse(JSON.stringify(edited))
 expect(readNativeColorTrack(roundTrip,'root',{clipId:'title'})).toEqual(readNativeColorTrack(edited,'root',{clipId:'title'}))
 for(const patch of [{side:'wrong',point:{x:0.5,y:1}},{side:'outgoing',point:{x:1.1,y:0}},{continuity:'wrong'},{}])expect(()=>applyNativeColorOperation(keyed,'root',{op:'edit_color_tangents',clipId:'title',tick:0,...patch})).toThrow()
})


it('uses retained Concat fill for reads, anchoring and deleting the final native color key',()=>{
 const input=project(),node=input.compositions.root!
 const unlocked={...input,compositions:{root:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>track.kind==='text'?{...track,locked:false,clips:track.clips.map(clip=>({...clip,concatTitle:{color:'#ff804080',fontWeight:700}}))}:{...track,locked:false})}}}}
 expect(readNativeColorTrack(unlocked,'root',{clipId:'title'})).toEqual({mode:'constant',value:{r:255,g:128,b:64,a:128}})
 const keyed=applyNativeColorOperation(unlocked,'root',{op:'set_color_key',clipId:'title',playheadTicks:1500000,value:{r:0,g:0,b:255,a:255}})
 expect(readNativeColorTrack(keyed,'root',{clipId:'title'})).toMatchObject({mode:'keyframes',keyframes:[{tick:0,value:{r:255,g:128,b:64,a:128}},{tick:500000,value:{r:0,g:0,b:255,a:255}}]})
 const one=applyNativeColorOperation(keyed,'root',{op:'delete_color_key',clipId:'title',tick:0})
 const constant=applyNativeColorOperation(one,'root',{op:'delete_color_key',clipId:'title',tick:500000})
 expect(readNativeColorTrack(constant,'root',{clipId:'title'})).toEqual({mode:'constant',value:{r:0,g:0,b:255,a:255}})
 expect(constant.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({concatTitle:{fontWeight:700,color:'#0000ffff'}})
})
