import {expect,it} from 'vitest'
import {initializeConcatTitleAnimation} from './concatTitleAnimation'
import {insertNativeConcatTitle} from './nativeTitleInsert'
import type {NativeNestedProject} from '$lib/api'
const canvas={width:320,height:180,fpsMilli:30000,backgroundMode:'color' as const,backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}
function clip(){const project:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas}}}};const result=insertNativeConcatTitle(project,'root',{text:'Title',clipId:'title',trackId:'',newTrackId:'titles',startUs:500000,durationUs:3000000}),track=result.compositions.root.content.tracks[0];if(track.kind!=='text')throw new Error('text required');return track.clips[0]}
it('preserves translated, rotated, nonuniformly scaled and translucent constant rest pose',()=>{
 const original=clip(),constant=(value:number)=>({mode:'constant' as const,value}),changed={...original,opacity:constant(0.4),transform:{...original.transform,x:constant(64),y:constant(-18),scaleX:constant(1.5),scaleY:constant(0.7),rotationDegrees:constant(23)}}
 expect(initializeConcatTitleAnimation(changed,canvas)).toEqual({durationTicks:3000000,keys:{},canvas:[320,180],stretch:[1.5,0.7],rest:{scaleX:1,scaleY:1,x:0.2,y:-0.1,rotationDegrees:23,opacity:0.4}});expect(changed.transform).not.toHaveProperty('concatAnimation')
})
it('refuses conversion of native keyframe channels instead of flattening them',()=>{
 const original=clip(),changed={...original,transform:{...original.transform,x:{mode:'keyframes' as const,track:{timeBase:1000000,interpolation:'linear' as const,keyframes:[{tick:0,value:0},{tick:1000000,value:100}]}}}}
 expect(()=>initializeConcatTitleAnimation(changed,canvas)).toThrow('Нативные ключи')
})
