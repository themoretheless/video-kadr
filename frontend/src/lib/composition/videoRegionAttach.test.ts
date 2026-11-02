import {expect,it} from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type {NativeNestedProject} from '$lib/api'
import {videoRegionRecipe} from '../components/composition/test-fixtures/videoRegionRecipe'
import {attachNativeVideoRegion} from './videoRegionAttach'
import {editNativeTrack} from './nativeTrackEdit'
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
it('atomically attaches independent resources to both linked members without changing clips or inputs',()=>{
 const {project,snapshot,region,original,mask}=setup(),before=JSON.stringify(project)
 const next=attachNativeVideoRegion(project,'new-timeline','auto-first',snapshot,original,mask,region)
 const node=next.compositions['new-timeline']!
 expect(node.content.tracks).toBe(project.compositions['new-timeline']!.content.tracks)
 for(const id of ['auto-first','auto-first-audio'])expect(node.content.metadata!.mediaVersions![id]!.videoCutout).toEqual({...snapshot,trackedRegions:[region]})
 expect(node.content.sources.tracked).toMatchObject({id:'tracked',width:256,height:256,hasAudio:false})
 expect(next.compositions.root).toBe(project.compositions.root);expect(JSON.stringify(project)).toBe(before)
 expect(attachNativeVideoRegion(next,'new-timeline','auto-first',node.content.metadata!.mediaVersions!['auto-first']!.videoCutout!,original,mask,region)).toBe(next)
})
it('refuses a locked companion and stale snapshot without partial source registration',()=>{
 const {project,snapshot,region,original,mask}=setup(),before=JSON.stringify(project)
 const locked=editNativeTrack(project,'new-timeline',{kind:'flag',id:'audio-2',flag:'locked',value:true})
 expect(()=>attachNativeVideoRegion(locked,'new-timeline','auto-first',snapshot,original,mask,region)).toThrow('заблокирована')
 expect(()=>attachNativeVideoRegion(project,'new-timeline','auto-first',{...snapshot,maskHash:'9'.repeat(64)},original,mask,region)).toThrow('изменилась')
 expect(JSON.stringify(project)).toBe(before)
})
it.each(['hash','clock','shape','count','mask','collision'])('refuses incompatible %s before publication',kind=>{
 const {project,snapshot,region,original,mask}=setup(),before=JSON.stringify(project)
 const changed={...region,...(kind==='hash'?{sourceHash:'9'.repeat(64)}:{}),...(kind==='clock'?{frameRate:{numerator:5,denominator:1}}:{}),...(kind==='shape'?{width:38}:{}),...(kind==='count'?{frames:5,trackedFrames:5}:{}),...(kind==='collision'?{maskSourceId:'source'}:{})}
 expect(()=>attachNativeVideoRegion(project,'new-timeline','auto-first',snapshot,original,kind==='mask'?{...mask,hasAudio:true}:kind==='collision'?{...mask,id:'source'}:mask,changed)).toThrow()
 expect(JSON.stringify(project)).toBe(before)
})
