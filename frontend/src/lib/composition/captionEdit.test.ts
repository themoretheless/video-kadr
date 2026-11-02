import {assertCompositionLocksPreserved} from './locks'
import {expect,test} from 'vitest'
import {createComposition} from './commands'
import {buildCompositionWireDocument} from './payload'
import {applyCaptionEdit} from './captionEdit'
import type {Composition} from './types'
test('applies native caption edit while retaining authoring metadata and unchanged ordinary tracks',()=>{
 const before:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),metadata:{sequenceName:'Scene'},captionTracks:[{id:'captions',name:'Captions',style:{size:54},clips:[{id:'cue',timelineStartTick:0,timelineEndTick:2000000,text:'Before',speaker:'Ann'}]}]}
 const after=buildCompositionWireDocument({...before,captionTracks:[{...before.captionTracks![0],style:{size:54,font:'Inter',align:'center'},clips:[{...before.captionTracks![0].clips[0],text:'After',speaker:null}]}]})
 const next=applyCaptionEdit(before,after)
 expect(next.captionTracks![0].clips[0]).toMatchObject({text:'After',speaker:null})
 expect(next.metadata).toEqual(before.metadata);expect(next.captionTracks![0].style).toEqual(before.captionTracks![0].style)
 expect(before.captionTracks![0].clips[0].text).toBe('Before')
 expect(()=>applyCaptionEdit(before,{...after,captionTracks:[]})).toThrow('дорожек')
})

test('native default fields do not change an untouched locked caption track',()=>{
 const before:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'locked',name:'Locked',locked:true,clips:[{id:'cue',timelineStartTick:0,timelineEndTick:2000000,text:'Keep'}]},{id:'editable',name:'Editable',clips:[{id:'other',timelineStartTick:0,timelineEndTick:2000000,text:'Before'}]}]}
 const after=buildCompositionWireDocument({...before,captionTracks:before.captionTracks!.map(track=>({...track,clips:track.clips.map(cue=>({...cue,speaker:null,settings:'',text:cue.id==='other'?'After':cue.text}))}))})
 const next=applyCaptionEdit(before,after)
 expect(next.captionTracks![0]).toEqual(before.captionTracks![0]);expect(()=>assertCompositionLocksPreserved(before,next)).not.toThrow()
 expect(next.captionTracks![1].clips[0].text).toBe('After')
})
