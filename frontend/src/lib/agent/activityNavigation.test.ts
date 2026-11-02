import {beforeEach,expect,it} from 'vitest'
import {activityNavigationTarget,revealActivityAffected} from './activityNavigation.svelte'
import {activityReveal} from './activityReveal.svelte'
import {agentActivity,resetAgentActivity} from './activity.svelte'
import {agentSession} from './session.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition} from '../state/nestedComposition.svelte'
import {buildCompositionWireDocument} from '../composition/payload'
import type {AffectedEntity} from './affectedEntities'
const ref=(kind:AffectedEntity['kind'],id:string,compositionId='ordinary'):AffectedEntity=>({kind,id,compositionId,label:'Historical name'})
beforeEach(()=>{
 closeNestedComposition();setCompositionStorageForTests(null);resetCompositionForTests();resetAgentActivity()
 compositionState.document={...compositionState.document,tracks:[{id:'text-track',kind:'text',name:'Captions',hidden:false,locked:false,clips:[{id:'title',kind:'text',timelineStartTicks:1000000,durationTicks:2000000,text:'Title',x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#FFFFFFFF',align:'center'}}]},{id:'empty-track',kind:'text',name:'Empty',hidden:false,locked:false,clips:[]}]}
})
it('prefers the first live layer over tracks without seeking, editing history, ending work or unlocking',()=>{
 compositionState.transport.playheadTicks=1700000;compositionState.transport.playing=true
 agentSession.view='agent';agentSession.work={id:'work',owner:'owner',kind:'ordinary',reason:'Work',checkpointId:'point',startedAt:1};agentSession.lock={owner:'owner',reason:'Locked',expiresAt:Date.now()+60000}
 const before=JSON.stringify(compositionState.document),revision=agentSession.revision,depth=compositionState.history.past.length
 expect(revealActivityAffected([ref('Track','empty-track'),ref('Layer','deleted'),ref('Layer','title')],agentActivity.workspaceId)).toBe(true)
 expect(compositionState.ui.selectedClipId).toBe('title');expect(compositionState.ui.selectedClipIds).toEqual(['title'])
 expect(compositionState.transport.playheadTicks).toBe(1700000);expect(compositionState.transport.playing).toBe(true)
 expect(JSON.stringify(compositionState.document)).toBe(before);expect(agentSession.revision).toBe(revision);expect(compositionState.history.past).toHaveLength(depth)
 expect(agentSession.work?.id).toBe('work');expect(agentSession.lock?.owner).toBe('owner');expect(agentSession.view).toBe('editor')
 expect(activityReveal.request).toMatchObject({mode:'ordinary',trackId:'text-track',clipId:'title'})
})
it('reveals a track without replacing the current clip selection',()=>{
 compositionState.ui.selectedClipId='title';compositionState.ui.selectedClipIds=['title'];compositionState.ui.selectedTrackId='text-track'
 expect(revealActivityAffected([ref('Layer','deleted'),ref('Track','empty-track')],agentActivity.workspaceId)).toBe(true)
 expect(activityReveal.request).toMatchObject({trackId:'empty-track',clipId:null})
 expect(compositionState.ui.selectedClipId).toBe('title');expect(compositionState.ui.selectedTrackId).toBe('text-track')
})
it('resolves repeated native IDs only in their recorded composition and preserves the native playhead',()=>{
 const wire=buildCompositionWireDocument(compositionState.document)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:wire,instances:[]},child:{content:wire,instances:[]}}}
 nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1750000
 expect(activityNavigationTarget([ref('Layer','title','missing')],agentActivity.workspaceId)).toBeNull()
 const before=JSON.stringify(nestedComposition.document)
 expect(revealActivityAffected([ref('Layer','title','child')],agentActivity.workspaceId)).toBe(true)
 expect(nestedComposition.activeCompositionId).toBe('child');expect(nestedComposition.timeUs).toBe(1750000)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toEqual([])
 expect(activityReveal.request).toMatchObject({mode:'nested',compositionId:'child',clipId:'title'})
 nestedHistory.busy=true
 expect(revealActivityAffected([ref('Layer','title','root')],agentActivity.workspaceId)).toBe(false)
 expect(nestedComposition.activeCompositionId).toBe('child')
})
it('refuses deleted/unsupported/stale-opening references and clears pending navigation on reset',()=>{
 expect(revealActivityAffected([ref('Layer','deleted'),ref('Marker','marker')],agentActivity.workspaceId)).toBe(false)
 expect(activityReveal.request).toBeNull()
 const previous=agentActivity.workspaceId
 expect(revealActivityAffected([ref('Layer','title')],previous)).toBe(true)
 resetAgentActivity();expect(activityReveal.request).toBeNull()
 expect(revealActivityAffected([ref('Layer','title')],previous)).toBe(false)
})
