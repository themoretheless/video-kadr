import type {NativeNestedProject} from '$lib/api'
import {createComposition,addTrack,addClip} from '../commands'
import {buildCompositionWireDocument} from '../payload'
export function scalarClipboardProject():NativeNestedProject{
 let doc=createComposition({width:128,height:96,fps:4,backgroundColor:'#000000FF'})
 for(const [id,start,x] of [['source',1000000,0],['target',2000000,80]] as const){
  doc=addTrack(doc,{id:`${id}-track`,kind:'text',name:id,locked:false,hidden:false,clips:[]})
  doc=addClip(doc,`${id}-track`,{id,kind:'text',text:id,timelineStartTicks:start,durationTicks:2000000,x,y:0,opacity:1,style:{fontSizePx:24,color:'#FFFFFFFF',align:'center'}})
 }
 const content=buildCompositionWireDocument(doc),source=content.tracks[0]!.clips[0]!
 if(!('transform' in source))throw new Error('Visual clip required')
 Object.assign(source,{enabled:false,opacity:{mode:'keyframes',track:{timeBase:24,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:12,value:0.5},{tick:24,value:1}]}}})
 Object.assign(source.transform,{x:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:750,value:20},{tick:1250,value:40}]}}})
 return {schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}}
}
