import {expect,test} from 'vitest'
import {createComposition,splitClip} from './commands'
import {pasteInsertComposition} from './clipboardInsert'
import type {Composition,TextClip} from './types'
const clip:TextClip={id:'clip',kind:'text',timelineStartTicks:0,durationTicks:4_000_000,text:'Text',x:0,y:0,opacity:1,style:{fontSizePx:24,color:'#ffffffff',align:'center'}}
const fixture=():Composition=>({...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),metadata:{sequenceName:'Copy sequence',clipNames:{clip:'Copied name'},dropFrame:false},tracks:[{id:'track',kind:'text',name:'Text',hidden:false,locked:false,clips:[clip]}]})
test('Paste Insert retains snapshot names and destination metadata while naming split remnants',()=>{
 const snapshot=fixture()
 const destination:Composition={...snapshot,metadata:{sequenceName:'Destination',clipNames:{clip:'Changed name'},dropFrame:true}}
 const frozen=JSON.stringify(destination)
 let id=0
 const result=pasteInsertComposition(destination,snapshot,['clip'],1_000_000,()=>`copy-${id++}`)
 expect(result.document.metadata).toMatchObject({sequenceName:'Destination',dropFrame:true,clipNames:{clip:'Changed name','copy-0':'Copied name','copy-1':'Changed name'}})
 expect(JSON.stringify(destination)).toBe(frozen)
 expect(snapshot.metadata!.clipNames).toEqual({clip:'Copied name'})
})
test('ordinary razor carries a named clip to the right piece without inventing unnamed metadata',()=>{
 const document=fixture()
 expect(splitClip(document,'clip',1_000_000,'right').metadata!.clipNames).toEqual({clip:'Copied name',right:'Copied name'})
 expect(splitClip({...document,metadata:undefined},'clip',1_000_000,'right').metadata).toBeUndefined()
})

test('Paste Insert restores used source names from snapshot and retains destination overrides',()=>{
 const base=fixture()
 const media={id:'video',kind:'video' as const,durationTicks:10_000_000,width:320,height:180,hasAudio:false}
 const snapshot:Composition={...base,sources:{video:media},metadata:{sourceNames:{video:'Camera A'},sequenceName:'Source sequence'},tracks:[{id:'video-track',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[{id:'video-clip',kind:'video',sourceId:'video',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:4_000_000,transform:{x:0,y:0,width:320,height:180,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1}]}]}
 const empty:Composition={...base,metadata:{sequenceName:'Destination'},tracks:[{id:'destination',kind:'video',name:'Destination',hidden:false,muted:false,locked:false,clips:[]}]}
 let id=0
 const restored=pasteInsertComposition(empty,snapshot,['video-clip'],0,()=>`restored-${id++}`).document
 expect(restored.sources.video).toEqual(media)
 expect(restored.sources.video).not.toBe(snapshot.sources.video)
 expect(restored.metadata).toMatchObject({sequenceName:'Destination',sourceNames:{video:'Camera A'}})
 const renamed={...empty,sources:{video:media},metadata:{sequenceName:'Destination',sourceNames:{video:'Current camera'}}}
 expect(pasteInsertComposition(renamed,snapshot,['video-clip'],0,()=>`renamed-${id++}`).document.metadata!.sourceNames!.video).toBe('Current camera')
 expect(snapshot.metadata!.sourceNames!.video).toBe('Camera A')
})
