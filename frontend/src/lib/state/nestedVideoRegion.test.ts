import {beforeEach,expect,it,vi} from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type {NativeNestedProject,SavedSamVideoRegion} from '$lib/api'
import * as api from '$lib/api'
import {videoRegionRecipe} from '../components/composition/test-fixtures/videoRegionRecipe'
import {attachNestedVideoRegion,closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from './nestedComposition.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
beforeEach(()=>{closeNestedComposition();vi.resetAllMocks()})
function setup(){
 const project=structuredClone(fixture.nativeExtendedHeads[0]!.after) as unknown as NativeNestedProject
 const node=project.compositions['new-timeline']!,base=node.content.sources.source!
 const region=videoRegionRecipe().projectOrigin!.snapshot.trackedRegions![0]!
 const original={...base,id:'original',width:37,height:23},mask={...base,id:'tracked',width:256,height:256,hasAudio:false}
 const snapshot={...videoRegionRecipe().projectOrigin!.snapshot,trackedRegions:[]}
 const sources={...node.content.sources,original,automatic:{...mask,id:'automatic',width:512,height:320}}
 const versions=Object.fromEntries(['auto-first','auto-first-audio'].map(id=>[id,{originalSourceId:'original',copySourceId:'source',videoCutout:snapshot}]))
 project.compositions['new-timeline']={...node,content:{...node.content,sources,metadata:{...node.content.metadata,mediaVersions:versions}}}
 return {project,snapshot,region,original,mask}
}
function entry():SavedSamVideoRegion{const {project,region}=setup();const input=project.compositions['new-timeline']!.content.sources.original!;const base={kind:'source' as const,filename:'region.mkv',url:'/private',createdAt:0,mediaType:'video' as const,duration:input.durationTicks/1000000,width:37,height:23};return{source:{...base,id:'original',...(input.hasAudio?{acodec:'pcm_s16le',audioChannels:input.audioChannels??2}:{})},mask:{...base,id:'tracked',width:256,height:256},snapshot:region}}
it('records one atomic inventory edit with Undo/Redo and preserved render bindings',async()=>{
 const {project,snapshot}=setup();nestedComposition.document=project;nestedComposition.renderedOutputs={root:'movie'}
 await attachNestedVideoRegion('new-timeline','auto-first',snapshot,entry())
 expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
 const next=JSON.parse(JSON.stringify(nestedComposition.document)) as NativeNestedProject
 expect(next!.compositions['new-timeline']!.content.metadata!.mediaVersions!['auto-first']!.videoCutout!.trackedRegions).toHaveLength(1)
 undoNestedComposition();expect(nestedComposition.document).toEqual(project);expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
 redoNestedComposition();expect(nestedComposition.document).toEqual(next)
})
it('refuses native validation failure without a history record or registered resources',async()=>{
 const {project,snapshot}=setup();nestedComposition.document=project;vi.mocked(api.evaluateNestedFrame).mockRejectedValue(new Error('invalid native project'))
 await expect(attachNestedVideoRegion('new-timeline','auto-first',snapshot,entry())).rejects.toThrow()
 expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document).toEqual(project)
})
