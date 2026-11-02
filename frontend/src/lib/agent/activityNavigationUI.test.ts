// @ts-expect-error Tests need the Svelte client entry.
import {mount,tick,unmount} from '../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it} from 'vitest'
import NestedClipTransfer from '../components/composition/NestedClipTransfer.svelte'
import NestedTrackControls from '../components/composition/NestedTrackControls.svelte'
import MultiTrackTimeline from '../components/composition/MultiTrackTimeline.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {closeNestedComposition,nestedComposition,nestedHistory} from '../state/nestedComposition.svelte'
import {agentActivity,resetAgentActivity} from './activity.svelte'
import {revealActivityAffected} from './activityNavigation.svelte'
import {buildCompositionWireDocument} from '../composition/payload'
import type {AffectedEntity} from './affectedEntities'
let target:HTMLDivElement,app:ReturnType<typeof mount>|null=null
const ref=(kind:AffectedEntity['kind'],id:string,compositionId='ordinary'):AffectedEntity=>({kind,id,compositionId,label:'Historical label'})
beforeEach(()=>{
 closeNestedComposition();setCompositionStorageForTests(null);resetCompositionForTests();resetAgentActivity()
 compositionState.document={...compositionState.document,tracks:[{id:'text-track',kind:'text',name:'Captions',hidden:false,locked:false,clips:[{id:'title',kind:'text',timelineStartTicks:1000000000,durationTicks:2000000,text:'Title',x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#FFFFFFFF',align:'center'}}]}]}
 target=document.createElement('div');document.body.append(target)
})
afterEach(async()=>{if(app)await unmount(app);app=null;target.remove();closeNestedComposition()})
it('reveals an offscreen virtualized ordinary clip and focuses it without seeking',async()=>{
 app=mount(MultiTrackTimeline,{target});await tick()
 compositionState.transport.playheadTicks=1700000
 expect(target.querySelector('.composition-clip[data-clip-id="title"]')).toBeNull()
 expect(revealActivityAffected([ref('Layer','title')],agentActivity.workspaceId)).toBe(true)
 await tick();await tick();await tick()
 const clip=target.querySelector<HTMLElement>('.composition-clip[data-clip-id="title"]')!
 expect(clip).not.toBeNull();expect(clip.classList.contains('selected')).toBe(true)
 expect(document.activeElement).toBe(clip);expect(compositionState.transport.playheadTicks).toBe(1700000)
})
it('selects and focuses a native clip after switching composition without moving the native playhead',async()=>{
 const content=buildCompositionWireDocument(compositionState.document)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},child:{content,instances:[]}}}
 nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1750000
 app=mount(NestedClipTransfer,{target});await tick()
 expect(revealActivityAffected([ref('Layer','title','child')],agentActivity.workspaceId)).toBe(true)
 await tick();await tick();await tick()
 const input=target.querySelector<HTMLInputElement>('[data-clip-id="title"]')!
 expect(input.checked).toBe(true);expect(document.activeElement).toBe(input)
 expect(nestedComposition.activeCompositionId).toBe('child');expect(nestedComposition.timeUs).toBe(1750000)
 expect(nestedHistory.past).toHaveLength(0)
})
it('reveals a native track through its existing editor controls without a content mutation',async()=>{
 const content=buildCompositionWireDocument(compositionState.document)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},child:{content,instances:[]}}}
 nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1750000
 app=mount(NestedTrackControls,{target});await tick()
 expect(revealActivityAffected([ref('Track','text-track','child')],agentActivity.workspaceId)).toBe(true)
 await tick();await tick()
 expect(document.activeElement?.getAttribute('aria-label')).toBe('Название дорожки text-track')
 expect(nestedComposition.activeCompositionId).toBe('child');expect(nestedComposition.timeUs).toBe(1750000)
 expect(nestedHistory.past).toHaveLength(0)
})
