import {beforeEach,expect,it} from 'vitest'
import {setNativeCaptionText} from './nativeCaptionText'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {importSrtToComposition} from '../state/composition/timeline.svelte'
import {buildCompositionWireDocument} from './payload'
import type {NativeNestedProject} from '../api'
let project:NativeNestedProject
beforeEach(()=>{
 setCompositionStorageForTests(null);resetCompositionForTests()
 importSrtToComposition('1\n00:00:01,000 --> 00:00:02,000\nOriginal')
 const content=buildCompositionWireDocument(compositionState.document)
 project={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},unused:{content,instances:[]}}}
})
it('changes only literal caption text, preserving clocks/style/animation/other compositions and input',()=>{
 const node=project.compositions.root!,track=node.content.tracks[0]!,clip=track.clips[0]!,before=JSON.stringify(project)
 const changed=setNativeCaptionText(project,'root',clip.id,'Исправлено <&>\nNext line','Original')
 expect(changed.compositions.root!.content.tracks[0]!.clips[0]).toEqual({...clip,text:'Исправлено <&>\nNext line'})
 expect(changed.compositions.unused).toBe(project.compositions.unused)
 expect(JSON.stringify(project)).toBe(before)
 expect(setNativeCaptionText(project,'root',clip.id,'Original','Original')).toBe(project)
})
it('refuses stale text, locks, normal titles and invalid content before any mutation',()=>{
 const node=project.compositions.root!,track=node.content.tracks[0]!,clip=track.clips[0]!,before=JSON.stringify(project)
 for(const text of ['','  ','Bad\0text','A'.repeat(513)])expect(()=>setNativeCaptionText(project,'root',clip.id,text,'Original')).toThrow()
 expect(()=>setNativeCaptionText(project,'root',clip.id,'New','Stale')).toThrow('изменился')
 const title={...project,compositions:{...project.compositions,root:{...node,content:{...node.content,metadata:{}}}}}
 expect(()=>setNativeCaptionText(title,'root',clip.id,'New','Original')).toThrow('не является')
 const locked={...project,compositions:{...project.compositions,root:{...node,content:{...node.content,tracks:[{...track,locked:true}]}}}}
 expect(()=>setNativeCaptionText(locked,'root',clip.id,'New','Original')).toThrow('заблокирована')
 expect(()=>setNativeCaptionText(project,'missing',clip.id,'New','Original')).toThrow()
 expect(JSON.stringify(project)).toBe(before)
})
it('counts Unicode code points without truncating surrogate pairs',()=>{
 const id=project.compositions.root!.content.tracks[0]!.clips[0]!.id
 expect(setNativeCaptionText(project,'root',id,'🙂'.repeat(512),'Original').compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({text:'🙂'.repeat(512)})
 expect(()=>setNativeCaptionText(project,'root',id,'🙂'.repeat(513),'Original')).toThrow()
})
