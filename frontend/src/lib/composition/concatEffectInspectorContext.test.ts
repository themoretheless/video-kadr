import {expect,it} from 'vitest'
import type {NativeNestedProject} from '$lib/api'
import {insertNativeConcatShape} from './nativeShapeInsert'
import {insertNativeConcatTitle} from './nativeTitleInsert'
import {insertNativeConcatTreatment} from './nativeTreatmentInsert'
import {mapNativeClipEffects} from './nativeEffectAuthoring'
import {curvePreviewProject} from './concatCurveAuthoring'
import {concatEffectInspectorTracks,concatEffectNativeKeyOrigin} from './concatEffectInspectorContext'
function project():NativeNestedProject{return {schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}}}
function documents(){const base={trackId:'',startUs:1000000,durationUs:3000000};let doc=insertNativeConcatShape(project(),'root',{...base,clipId:'shape',newTrackId:'shapes',kind:'arrow'});doc=insertNativeConcatTitle(doc,'root',{...base,clipId:'title',newTrackId:'titles',text:'Title'});return insertNativeConcatTreatment(doc,'root',{...base,clipId:'look',newTrackId:'look',effects:[{kind:'concat_exposure',parameters:{stops:1}}]})}
it('includes retained title/figure/treatment clips but excludes ordinary graphic and text ownership',()=>{
 const doc=documents(),node=doc.compositions.root,tracks=[...node.content.tracks];for(const track of node.content.tracks){if(track.kind==='text')tracks.push({...track,id:'ordinary-text',clips:track.clips.map(clip=>{const copy={...clip,id:'ordinary-title'};delete copy.concatTitle;return copy})});if(track.kind==='graphic'&&track.clips[0]?.concatShape)tracks.push({...track,id:'ordinary-graphic',clips:track.clips.map(clip=>{const copy={...clip,id:'ordinary-shape'};delete copy.concatShape;return copy})})}node.content={...node.content,tracks};expect(concatEffectInspectorTracks(doc,'root').flatMap(track=>track.clips.map(clip=>clip.id)).sort()).toEqual(['look','shape','title']);expect(concatEffectInspectorTracks(doc,'constructor')).toEqual([])
})
it('uses absolute treatment time, local title time and retained graphic trim time for native keys',()=>{
 const tracks=documents().compositions.root.content.tracks,look=tracks.find(track=>track.id==='look'),shape=tracks.find(track=>track.id==='shapes'),title=tracks.find(track=>track.id==='titles');if(look?.kind!=='graphic'||shape?.kind!=='graphic'||title?.kind!=='text')throw new Error('retained clips required');expect(concatEffectNativeKeyOrigin(look.clips[0])).toBe(0);expect(concatEffectNativeKeyOrigin(title.clips[0])).toBe(1000000);expect(concatEffectNativeKeyOrigin({...shape.clips[0],animationOffsetUs:250000})).toBe(750000)
})

it('builds nonmutating curve preview documents for retained text, shapes and treatments',()=>{
 for(const [trackId,clipId] of [['titles','title'],['shapes','shape'],['look','look']]){
  const base=mapNativeClipEffects(documents(),'root',trackId,clipId,()=>[{kind:'concat_curves',parameters:{}}]),before=JSON.stringify(base),points=[[0,0],[1,0.5]] as const
  const preview=curvePreviewProject(base,'root',trackId,clipId,0,'luma',points),track=preview.compositions.root.content.tracks.find(track=>track.id===trackId)!,clip=track.clips[0];if(!('effects' in clip))throw new Error('effect target required');expect(clip.effects?.[0]).toMatchObject({kind:'concat_curves',parameters:{luma:points}});expect(JSON.stringify(base)).toBe(before)
 }
})
