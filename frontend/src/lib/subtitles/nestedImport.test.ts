import {expect,it} from 'vitest'
import {createComposition} from '../composition/commands'
import {buildCompositionWireDocument,textClipToWire} from '../composition/payload'
import {MAX_COMPOSITION_TRACKS} from '../composition/types'
import {importNestedSubtitles} from './nestedImport'
import {cuesToTextClips} from './srt'
import {parseSubtitleDocument} from './import'
import type {NativeNestedProject} from '../api'
const base=createComposition({width:320,height:180,fps:24,backgroundColor:'#000000FF'})
function project():NativeNestedProject{
 const content=buildCompositionWireDocument(base)
 return {schemaVersion:1,root:'root',compositions:{root:{durationUs:1_000_000,content,instances:[]},unused:{content,instances:[]}}}
}
const body='1\n00:00:01,125 --> 00:00:04,000\nLong\n\n2\n00:00:02,000 --> 00:00:03,000\nInner\n\n3\n00:00:03,000 --> 00:00:04,000\nAdjacent'
function allocator(){let serial=0;return(prefix:string)=>`${prefix}-${++serial}`}
it('packs overlapping cues, shares adjacent spans and uses canonical text wire transport',()=>{
 const input=project(),before=JSON.stringify(input)
 const result=importNestedSubtitles(input,'root',body,allocator())
 const node=result.document.compositions.root!,tracks=node.content.tracks
 expect(result.count).toBe(3)
 expect(tracks.map(track=>track.clips.length)).toEqual([1,2])
 expect(node.durationUs).toBe(4_000_000)
 const first=tracks[0]!.clips[0]!
 const expected=textClipToWire(base,cuesToTextClips(parseSubtitleDocument(body).slice(0,1),()=>first.id)[0]!)
 expect(first).toEqual(expected)
 expect(node.content.metadata?.captionTextTracks).toEqual(tracks.map(track=>track.id))
 expect(result.document.compositions.unused).toBe(input.compositions.unused)
 expect(JSON.stringify(input)).toBe(before)
 const reuse=importNestedSubtitles(result.document,'root','WEBVTT\n\n00:04.000 --> 00:05.000\nReuse',allocatorFor(result.document))
 expect(reuse.document.compositions.root!.content.tracks).toHaveLength(2)
 expect(reuse.firstTrackId).toBe(tracks[0]!.id)
})
function allocatorFor(input:NativeNestedProject){void input;let n=100;return(prefix:string)=>`${prefix}-${++n}`}
it('keeps ordinary titles separate and skips locked caption lanes without touching them',()=>{
 const first=importNestedSubtitles(project(),'root',body,allocator()).document
 const node=first.compositions.root!,caption=node.content.tracks[0]!
 const title={...caption,id:'title-track',name:'Titles',clips:[]}
 const input={...first,compositions:{...first.compositions,root:{...node,content:{...node.content,tracks:[title,...node.content.tracks.map(track=>({...track,locked:true}))]}}}}
 const before=JSON.stringify(input)
 const imported=importNestedSubtitles(input,'root','1\n00:00:01,000 --> 00:00:02,000\nNew',allocatorFor(input))
 expect(imported.document.compositions.root!.content.tracks).toHaveLength(4)
 expect(imported.document.compositions.root!.content.tracks[0]).toBe(title)
 expect(imported.document.compositions.root!.content.tracks[1]).toEqual(input.compositions.root!.content.tracks[1])
 expect(JSON.stringify(input)).toBe(before)
})
it('supports ASS text/time and a safe cursor offset, refusing invalid input/ids/track overflow atomically',()=>{
 const input=project(),before=JSON.stringify(input)
 const result=importNestedSubtitles(input,'root','[Events]\nFormat: Start, End, Text\nDialogue: 0:00:01.12,0:00:02.50,Привет',allocator(),2_000_000)
 expect(result.document.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({timelineStartTick:3_120_000,timelineEndTick:4_500_000,text:'Привет'})
 expect(()=>importNestedSubtitles(input,'root',body,()=> 'same')).toThrow('уникальный')
 expect(()=>importNestedSubtitles(input,'root',body,allocator(),-1)).toThrow()
 expect(()=>importNestedSubtitles(input,'root','bad',allocator())).toThrow()
 expect(()=>importNestedSubtitles(input,'missing',body,allocator())).toThrow()
 const full={...input,compositions:{...input.compositions,root:{...input.compositions.root!,content:{...input.compositions.root!.content,tracks:Array.from({length:MAX_COMPOSITION_TRACKS},(_,i)=>({kind:'text' as const,id:`title-${i}`,name:'Title',hidden:false,locked:false,clips:[]}))}}}}
 expect(()=>importNestedSubtitles(full,'root',body,allocator())).toThrow('лимит дорожек')
 expect(JSON.stringify(input)).toBe(before)
})
