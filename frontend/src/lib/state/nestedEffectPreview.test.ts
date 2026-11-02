import{afterEach,expect,it}from'vitest'
import{nestedEffectPreview,clearEffectPreview,resolveEffectPreview}from'./nestedEffectPreview.svelte'
import type{NativeNestedProject}from'$lib/api'
const base:NativeNestedProject={schemaVersion:1,root:'root',compositions:{}}
afterEach(()=>nestedEffectPreview.value=null)
it('isolates drafts by document identity, time, composition and account session',()=>{const owner=Symbol(),gesture={};const project={...base};nestedEffectPreview.value={owner,gesture,base,project,timeUs:10,compositionId:'root',token:'first'};expect(resolveEffectPreview(base,10,undefined,'first')?.project).toBe(project);expect(resolveEffectPreview({...base},10,'root','first')).toBeNull();expect(resolveEffectPreview(base,11,'root','first')).toBeNull();expect(resolveEffectPreview(base,10,'other','first')).toBeNull();expect(resolveEffectPreview(base,10,'root','second')).toBeNull()})
it('late cleanup cannot clear another owner or a newer gesture',()=>{const owner=Symbol(),gesture={};nestedEffectPreview.value={owner,gesture,base,project:base,timeUs:0,compositionId:'root',token:''};clearEffectPreview(Symbol());expect(nestedEffectPreview.value).not.toBeNull();clearEffectPreview(owner,{});expect(nestedEffectPreview.value).not.toBeNull();clearEffectPreview(owner,gesture);expect(nestedEffectPreview.value).toBeNull()})
