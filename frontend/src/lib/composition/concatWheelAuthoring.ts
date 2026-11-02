import{mapNativeClipEffects}from'./nativeEffectAuthoring'
import type {WireVideoEffect,WireAnimatableValue} from './types'
import {sampleAnimatableValue} from './keyframes'
export type Wheel='lift'|'gamma'|'gain'
export type Axis='x'|'y'|'m'
export type WheelEffect=Extract<WireVideoEffect,{kind:'concat_color_wheels'|'concat_adjust'}>
const axes=['x','y','m']as const
function localTime(time:number){if(!Number.isSafeInteger(time)||time<0)throw new Error('Курсор должен находиться после начала клипа')}
function coordinate(value:number){if(!Number.isFinite(value)||value< -1||value>1)throw new Error('Координата колеса вне диапазона')}
export function sampleWheel(effect:WheelEffect,wheel:Wheel,timeUs:number):[number,number,number]{
 return axes.map((axis,i)=>{const curve=effect.animation?.[`${wheel}.${axis}`];return curve?sampleAnimatableValue(curve,timeUs):effect.parameters[wheel]?.[i]??0})as[number,number,number]
}
function withKey(previous:WireAnimatableValue|undefined,timeUs:number,value:number):WireAnimatableValue{
 localTime(timeUs);coordinate(value)
 const timeBase=previous?.mode==='keyframes'?previous.track.timeBase:1000000
 const tick=Math.round(timeUs*timeBase/1000000)
 if(!Number.isSafeInteger(timeBase)||timeBase<=0||!Number.isSafeInteger(tick))throw new Error('Невозможно представить время ключа')
 const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
 const existing=points.find(p=>p.tick===tick);const previousPoint=points.filter(p=>p.tick<tick).at(-1)
 const shape=existing??(previousPoint?.interpolation?{interpolation:previousPoint.interpolation}:{})
 const keyframes=[...points.filter(p=>p.tick!==tick),{...shape,tick,value}].sort((a,b)=>a.tick-b.tick)
 return{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase,interpolation:'linear',keyframes}}
}
/** Key the source wheel as one knob, sampling before modifying any of its curves. */
export function keyWheel<E extends WheelEffect>(effect:E,wheel:Wheel,timeUs:number,authored?:readonly[number,number,number]):E{
 localTime(timeUs);const values=authored??sampleWheel(effect,wheel,timeUs);const animation={...effect.animation}
 for(const [i,axis]of axes.entries()){const channel=`${wheel}.${axis}`as const;animation[channel]=withKey(effect.animation?.[channel],timeUs,values[i]!)}
 return{...effect,animation}
}
/** An existing ride gets a key; an unkeyed coordinate keeps constant authoring. */
export function setWheel<E extends WheelEffect>(effect:E,wheel:Wheel,changes:Partial<Record<Axis,number>>,timeUs:number):E{
 localTime(timeUs);const parameters={...effect.parameters};const values:[number,number,number]=[...(parameters[wheel]??[0,0,0])];const animation={...effect.animation};let changedBase=false
 for(const [i,axis]of axes.entries()){const value=changes[axis];if(value===undefined)continue;coordinate(value);const channel=`${wheel}.${axis}`as const;const previous=effect.animation?.[channel];if(previous?.mode==='keyframes')animation[channel]=withKey(previous,timeUs,value);else if(previous?.mode==='constant')animation[channel]={...previous,value};else{values[i]=value;changedBase=true}}
 if(changedBase)parameters[wheel]=values
 return{...effect,parameters,...(Object.keys(animation).length?{animation}:{})}
}
/** Preview copy only: source references/other clips remain owned by the base document. */
export function wheelPreviewProject(base:import('$lib/api').NativeNestedProject,compositionId:string,trackId:string,clipId:string,index:number,wheel:Wheel,changes:Partial<Record<Axis,number>>,timeUs:number,kind:'concat_color_wheels'|'concat_adjust'='concat_color_wheels'):import('$lib/api').NativeNestedProject{
 return mapNativeClipEffects(base,compositionId,trackId,clipId,effects=>{const effect=effects[index];if(!effect||(effect.kind!=='concat_color_wheels'&&effect.kind!=='concat_adjust')||effect.kind!==kind)throw new Error('Цветовое колесо не найдено');return effects.map((e,i)=>i===index?setWheel(effect,wheel,changes,timeUs):e)})
}
