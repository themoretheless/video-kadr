import {videoRegionRecipe} from '../components/composition/test-fixtures/videoRegionRecipe'
import {expect,it} from 'vitest'
import {createComposition,registerSource,addTrack,addClip,duplicateClip} from './commands'
import {buildCompositionWireDocument} from './payload'
import {validateComposition,normalizeComposition} from './validation'
import {mediaVersionSourceIds,type Composition,type VideoMaskAttachment} from './types'
import {relinkCompositionSource} from './relink'
import {createCompositionTemplate,instantiateCompositionTemplate,parseCompositionTemplate,serializeCompositionTemplate} from './templates'
import {pasteInsertComposition} from './clipboardInsert'
function fixture():Composition {
 let value=createComposition({width:74,height:46,fps:4,backgroundColor:'#000000'})
 for(const id of ['original','copy','masks'])value=registerSource(value,{id,kind:'video',durationTicks:1000000,width:id==='masks'?512:37,height:id==='masks'?320:23,hasAudio:false})
 value=addTrack(value,{id:'video',kind:'video',name:'Video',locked:false,hidden:false,muted:false,clips:[]})
 value=addClip(value,'video',{id:'clip',kind:'video',sourceId:'copy',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,speed:1,transform:{x:0,y:0,width:74,height:46,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1,audioPan:0})
 const snapshot:VideoMaskAttachment={sourceId:'original',maskSourceId:'masks',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),width:37,height:23,maskWidth:512,maskHeight:320,frames:4,frameRate:{numerator:4,denominator:1},corrections:[{frame:1,correction:{custom:true,strokes:[{tool:'eraser',size:.1,points:[[.5,.5]],at:null}],feather:0}}]}
 return normalizeComposition({...value,metadata:{mediaVersions:{clip:{originalSourceId:'original',copySourceId:'copy',videoCutout:snapshot}}}})
}
it('retains original and mask resources through wire, template, duplicate and clipboard ownership paths',()=>{
 const value=fixture(),snapshot=value.metadata!.mediaVersions!.clip!.videoCutout
 expect(validateComposition(value)).toEqual([]);expect(mediaVersionSourceIds(value.metadata?.mediaVersions?.clip).sort()).toEqual(['copy','masks','original'])
 expect(buildCompositionWireDocument(value).metadata?.mediaVersions?.clip?.videoCutout).toEqual(snapshot)
 const restored=instantiateCompositionTemplate(parseCompositionTemplate(serializeCompositionTemplate(createCompositionTemplate('video-masks','Masks',value,[]))),{})
 expect(restored.sources.masks).toEqual(value.sources.masks);expect(restored.metadata?.mediaVersions?.clip?.videoCutout).toEqual(snapshot)
 const target=addTrack(createComposition({width:74,height:46,fps:4,backgroundColor:'#000000'}),{id:'video',kind:'video',name:'Video',locked:false,hidden:false,muted:false,clips:[]});let n=0;const pasted=pasteInsertComposition(target,value,['clip'],0,()=>`video-mask-${n++}`).document
 expect(pasted.sources.masks).toEqual(value.sources.masks);expect(Object.values(pasted.metadata?.mediaVersions??{})[0]?.videoCutout).toEqual(snapshot)
 const duplicate=duplicateClip(value,'clip',{id:'clone',timelineStartTicks:1000000});expect(duplicate.metadata?.mediaVersions?.clone?.videoCutout).toEqual(snapshot)
})
it('relinks source IDs inside the snapshot and refuses missing, audible or mismatched mask resources',()=>{
 const value=fixture();const relinked=relinkCompositionSource(value,'masks',{...value.sources.masks!,id:'new-mask'});expect(relinked.metadata?.mediaVersions?.clip?.videoCutout?.maskSourceId).toBe('new-mask')
 for(const change of ['missing','audible','shape','recipe']){const broken=JSON.parse(JSON.stringify(value));if(change==='missing')delete broken.sources.masks;else if(change==='audible')broken.sources.masks.hasAudio=true;else if(change==='shape')broken.sources.masks.width=511;else broken.metadata.mediaVersions.clip.videoCutout.corrections[0].frame=4;expect(validateComposition(broken).length).toBeGreaterThan(0)}
})

it('retains temporal SAM resources and provenance through templates, clipboard, duplication and relinking',()=>{
 let value=registerSource(fixture(),{id:'sam-region',kind:'video',durationTicks:1000000,width:256,height:256,hasAudio:false})
 const snapshot=value.metadata!.mediaVersions!.clip!.videoCutout!
 const region={sourceId:'original',maskSourceId:'sam-region',sourceHash:snapshot.sourceHash,maskHash:'d'.repeat(64),maskFileHash:'e'.repeat(64),encoderHash:'f'.repeat(64),decoderHash:'0'.repeat(64),runtimeHash:'1'.repeat(64),promptHash:'8e0f6ce7be723f24b418232b01f7fdb4ddac66f10dcab24c069812ec3aa39e62',points:[[.5,.5]] as [number,number][],width:37,height:23,frames:4,frameRate:{numerator:4,denominator:1},seedFrame:1,trackedFrames:4,forwardLostAt:null,backwardLostAt:null}
 value=normalizeComposition({...value,metadata:{...value.metadata,mediaVersions:{clip:{...value.metadata!.mediaVersions!.clip!,videoCutout:{...snapshot,trackedRegions:[region]}}}}})
 expect(validateComposition(value)).toEqual([])
 expect(mediaVersionSourceIds(value.metadata?.mediaVersions?.clip)).toContain('sam-region')
 expect(buildCompositionWireDocument(value).metadata?.mediaVersions?.clip?.videoCutout?.trackedRegions).toEqual([region])
 const restored=instantiateCompositionTemplate(parseCompositionTemplate(serializeCompositionTemplate(createCompositionTemplate('tracked-regions','Regions',value,[]))),{})
 expect(restored.sources['sam-region']).toEqual(value.sources['sam-region']);expect(restored.metadata?.mediaVersions?.clip?.videoCutout?.trackedRegions).toEqual([region])
 const target=addTrack(createComposition({width:74,height:46,fps:4,backgroundColor:'#000000'}),{id:'video',kind:'video',name:'Video',locked:false,hidden:false,muted:false,clips:[]});let n=0
 const pasted=pasteInsertComposition(target,value,['clip'],0,()=>`region-${n++}`).document
 expect(pasted.sources['sam-region']).toEqual(value.sources['sam-region']);expect(Object.values(pasted.metadata?.mediaVersions??{})[0]?.videoCutout?.trackedRegions).toEqual([region])
 expect(duplicateClip(value,'clip',{id:'clone',timelineStartTicks:1000000}).metadata?.mediaVersions?.clone?.videoCutout?.trackedRegions).toEqual([region])
 const relinked=relinkCompositionSource(value,'sam-region',{...value.sources['sam-region']!,id:'new-region'});expect(relinked.metadata?.mediaVersions?.clip?.videoCutout?.trackedRegions?.[0]?.maskSourceId).toBe('new-region')
 for(const mode of ['missing','audio','shape','clock','source','loss','duplicate','unknown']){const broken=JSON.parse(JSON.stringify(value));const retained=broken.metadata.mediaVersions.clip.videoCutout.trackedRegions
  if(mode==='missing')delete broken.sources['sam-region'];else if(mode==='audio')broken.sources['sam-region'].hasAudio=true;else if(mode==='shape')broken.sources['sam-region'].width=255;else if(mode==='clock')retained[0].frameRate.denominator=2;else if(mode==='source')retained[0].sourceHash='b'.repeat(64);else if(mode==='loss')retained[0].forwardLostAt=1;else if(mode==='duplicate')retained.push(retained[0]);else retained[0].path='/private'
  expect(validateComposition(broken).length,mode).toBeGreaterThan(0)
 }
})

it('relinks bound frame annotations together with inventory IDs without mutating the source document',()=>{
 const recipe=videoRegionRecipe();let value=registerSource(fixture(),{id:'tracked',kind:'video',durationTicks:1000000,width:256,height:256,hasAudio:false})
 const pair=value.metadata!.mediaVersions!.clip!;value=normalizeComposition({...value,metadata:{...value.metadata,mediaVersions:{clip:{...pair,videoCutout:{...pair.videoCutout!,corrections:recipe.corrections,trackedRegions:recipe.projectOrigin!.snapshot.trackedRegions}}}}})
 const before=JSON.stringify(value)
 for(const id of ['original','tracked']){const replacement=`relinked-${id}`,next=relinkCompositionSource(value,id,{...value.sources[id]!,id:replacement}),snapshot=next.metadata!.mediaVersions!.clip!.videoCutout!
  expect(validateComposition(next)).toEqual([])
  expect(id==='tracked'?snapshot.trackedRegions![0]!.maskSourceId:snapshot.trackedRegions![0]!.sourceId).toBe(replacement)
  expect(id==='tracked'?snapshot.corrections[0]!.correction.strokes[0]!.region!.maskSourceId:snapshot.corrections[0]!.correction.strokes[0]!.region!.sourceId).toBe(replacement)
 }
 expect(JSON.stringify(value)).toBe(before)
})
