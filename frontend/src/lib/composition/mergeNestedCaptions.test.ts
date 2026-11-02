import { beforeEach,expect,it } from 'vitest'
import { mergeNestedCaptions } from './mergeCaptions'
import { buildCompositionWireDocument } from './payload'
import { MAX_TEXT_LENGTH } from './types'
import type {NativeNestedProject} from '../api'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {importSrtToComposition} from '../state/composition/timeline.svelte'
let project:NativeNestedProject
beforeEach(()=>{
 setCompositionStorageForTests(null);resetCompositionForTests()
 importSrtToComposition('1\n00:00:01,000 --> 00:00:02,000\nOne\n\n2\n00:00:02,000 --> 00:00:03,000\nTwo\n\n3\n00:00:04,000 --> 00:00:05,000\nThree')
 const content=buildCompositionWireDocument(compositionState.document)
 project={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},unused:{content:JSON.parse(JSON.stringify(content)),instances:[]}}}
})
function lane(){const track=project.compositions.root!.content.tracks[0]!;if(track.kind!=='text')throw new Error('text required');return track}
it('unions a gap into the earliest native caption, preserving style/animation and other compositions',()=>{
 const track=lane(),[,two,three]=track.clips,before=JSON.stringify(project)
 const result=mergeNestedCaptions(project,'root',[three!.id,two!.id])
 expect(result.record).toMatchObject({layer_id:two!.id,t_start_us:2_000_000,t_end_us:5_000_000,content:'Two\nThree',merged:2,removed:[three!.id]})
 const merged=result.document.compositions.root!.content.tracks[0]!
 expect(merged.clips[1]).toEqual({...two,text:'Two\nThree',timelineEndTick:5_000_000})
 expect(result.document.compositions.unused).toBe(project.compositions.unused)
 expect(JSON.stringify(project)).toBe(before)
})
it('refuses a union over an unselected cue, locks, missing subjects and oversized joined text',()=>{
 const track=lane(),[one,two,three]=track.clips,before=JSON.stringify(project)
 expect(()=>mergeNestedCaptions(project,'root',[one!.id,three!.id])).toThrow('пересекается')
 expect(()=>mergeNestedCaptions(project,'root',[one!.id,'missing'])).toThrow()
 expect(()=>mergeNestedCaptions(project,'missing',[one!.id,two!.id])).toThrow()
 expect(()=>mergeNestedCaptions(project,'root',[one!.id,one!.id])).toThrow()
 const locked={...project,compositions:{...project.compositions,root:{...project.compositions.root!,content:{...project.compositions.root!.content,tracks:[{...track,locked:true}]}}}}
 expect(()=>mergeNestedCaptions(locked,'root',[one!.id,two!.id])).toThrow('заблокирована')
 const long={...project,compositions:{...project.compositions,root:{...project.compositions.root!,content:{...project.compositions.root!.content,tracks:[{...track,clips:track.clips.map(clip=>({...clip,text:'A'.repeat(MAX_TEXT_LENGTH)}))}]}}}}
 expect(()=>mergeNestedCaptions(long,'root',[one!.id,two!.id])).toThrow('лимит')
 expect(JSON.stringify(project)).toBe(before)
})
it('cleans removed caption metadata and links without deleting linked ordinary titles',()=>{
 const track=lane(),[,two,three]=track.clips
 const content=project.compositions.root!.content
 const title={...track,id:'title-track',clips:[{...track.clips[0]!,id:'title'}]}
 const input={...project,compositions:{...project.compositions,root:{...project.compositions.root!,content:{...content,tracks:[track,title],metadata:{...content.metadata,clipNames:{[three!.id]:'Removed',title:'Keep'},clipLinks:{pair:[three!.id,'title']}}}}}}
 const result=mergeNestedCaptions(input,'root',[two!.id,three!.id]).document.compositions.root!.content
 expect(result.tracks[1]!.clips[0]!.id).toBe('title')
 expect(result.metadata?.clipNames).toEqual({title:'Keep'})
 expect(result.metadata?.clipLinks).toEqual({})
 expect(()=>mergeNestedCaptions(input,'root',[two!.id,'title'])).toThrow()
})
