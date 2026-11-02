import {beforeEach,expect,it} from 'vitest'
import {createComposition,registerSource,addTrack,addClip,duplicateClip,setClipLocked} from './commands'
import {normalizeComposition} from './validation'
import type {Composition,VideoMaskAttachment} from './types'
import {createCompositionLink} from './links'
import {attachCompositionVideoRegion} from './videoRegionAttach'
import {videoRegionRecipe} from '../components/composition/test-fixtures/videoRegionRecipe'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {attachCompositionVideoRegionFromLibrary} from '../state/composition/media.svelte'
import {undoComposition,redoComposition} from '../state/composition/transport.svelte'
import type {SavedSamVideoRegion} from '$lib/api'
beforeEach(()=>{setCompositionStorageForTests(null);resetCompositionForTests()})
function fixture():Composition {
 let value=createComposition({width:74,height:46,fps:4,backgroundColor:'#000000'})
 for(const id of ['original','copy','masks'])value=registerSource(value,{id,kind:'video',durationTicks:1000000,width:id==='masks'?512:37,height:id==='masks'?320:23,hasAudio:false})
 value=addTrack(value,{id:'video',kind:'video',name:'Video',locked:false,hidden:false,muted:false,clips:[]})
 value=addClip(value,'video',{id:'clip',kind:'video',sourceId:'copy',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,speed:1,transform:{x:0,y:0,width:74,height:46,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1,audioPan:0})
 const snapshot:VideoMaskAttachment={sourceId:'original',maskSourceId:'masks',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),width:37,height:23,maskWidth:512,maskHeight:320,frames:4,frameRate:{numerator:4,denominator:1},corrections:[{frame:1,correction:{custom:true,strokes:[{tool:'eraser',size:.1,points:[[.5,.5]],at:null}],feather:0}}]}
 return normalizeComposition({...value,metadata:{mediaVersions:{clip:{originalSourceId:'original',copySourceId:'copy',videoCutout:snapshot}}}})
}
function setup(){
 let document=duplicateClip(fixture(),'clip',{id:'linked',timelineStartTicks:1000000})
 document=createCompositionLink(document,'link',['clip','linked'])
 const snapshot=document.metadata!.mediaVersions!.clip!.videoCutout!
 const region={...videoRegionRecipe().projectOrigin!.snapshot.trackedRegions![0]!,maskSourceId:'new-region'}
 const source=document.sources.original!,mask={...source,id:'new-region',width:256,height:256,hasAudio:false}
 return {document,snapshot,region,source,mask}
}
it('adds a portable region to every linked member without moving clips or modifying input',()=>{
 const {document,snapshot,region,source,mask}=setup(),before=JSON.stringify(document)
 const next=attachCompositionVideoRegion(document,'clip',snapshot,source,mask,region)
 expect(next.tracks).toBe(document.tracks);expect(next.links).toEqual(document.links)
 for(const id of ['clip','linked'])expect(next.metadata!.mediaVersions![id]!.videoCutout).toEqual({...snapshot,trackedRegions:[region]})
 expect(next.sources['new-region']).toEqual(mask);expect(JSON.stringify(document)).toBe(before)
 expect(attachCompositionVideoRegion(next,'clip',next.metadata!.mediaVersions!.clip!.videoCutout!,source,mask,region)).toBe(next)
})
it.each(['clip','track','stale','collision'])('refuses %s atomically',kind=>{
 const {document,snapshot,region,source,mask}=setup()
 const altered=kind==='clip'?setClipLocked(document,'linked',true):kind==='track'?{...document,tracks:document.tracks.map(t=>({...t,locked:true}))}:document
 const before=JSON.stringify(altered)
 expect(()=>attachCompositionVideoRegion(altered,'clip',kind==='stale'?{...snapshot,maskHash:'9'.repeat(64)}:snapshot,source,kind==='collision'?{...mask,id:'copy'}:mask,kind==='collision'?{...region,maskSourceId:'copy'}:region)).toThrow()
 expect(JSON.stringify(altered)).toBe(before)
})
it('commits resources and private media bindings as one undoable edit',()=>{
 const {document,snapshot,region}=setup();compositionState.document=document
 const before=JSON.parse(JSON.stringify(compositionState.document)),bindings=JSON.parse(JSON.stringify(compositionState.media))
 const base={kind:'source' as const,filename:'region.mkv',url:'/private',createdAt:0,mediaType:'video' as const,duration:1,width:37,height:23}
 const entry:SavedSamVideoRegion={source:{...base,id:'original'},mask:{...base,id:'new-region',width:256,height:256},snapshot:region}
 attachCompositionVideoRegionFromLibrary('clip',snapshot,entry)
 expect(compositionState.history.past).toHaveLength(1);expect(compositionState.media['new-region']!.url).toBe('/private')
 const next=JSON.parse(JSON.stringify(compositionState.document)),media=JSON.parse(JSON.stringify(compositionState.media))
 undoComposition();expect(compositionState.document).toEqual(before);expect(compositionState.media).toEqual(bindings)
 redoComposition();expect(compositionState.document).toEqual(next);expect(compositionState.media).toEqual(media)
 attachCompositionVideoRegionFromLibrary('clip',compositionState.document.metadata!.mediaVersions!.clip!.videoCutout!,entry);expect(compositionState.history.past).toHaveLength(1)
})
