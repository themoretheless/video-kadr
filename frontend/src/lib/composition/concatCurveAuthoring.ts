import type{NativeNestedProject}from'$lib/api'
import{mapNativeClipEffects}from'./nativeEffectAuthoring'
export type CurvePoints=readonly(readonly[number,number])[]
export type CurveChannel='luma'|'red'|'green'|'blue'
function unit(value:number){if(!Number.isFinite(value))throw new Error('Некорректная координата кривой');return Math.max(0,Math.min(1,value))}
function indexAt(points:CurvePoints,index:number){if(!Number.isSafeInteger(index)||index<0||index>=points.length)throw new Error('Точка кривой не найдена')}
export function moveCurvePoint(points:CurvePoints,index:number,x:number,y:number):CurvePoints{indexAt(points,index);const px=unit(x),py=unit(y);return points.map((p,i)=>i===index?[px,py]:p)}
export function addCurvePoint(points:CurvePoints,x:number,y:number):CurvePoints{if(points.length>=8)return points;const base:CurvePoints=points.length<2?[[0,0],[1,1]]:points;return[...base,[unit(x),unit(y)]]}
export function removeCurvePoint(points:CurvePoints,index:number):CurvePoints{indexAt(points,index);const kept=points.filter((_,i)=>i!==index);return kept.length<2?[]:kept}
export function curvePreviewProject(base:NativeNestedProject,compositionId:string,trackId:string,clipId:string,index:number,channel:CurveChannel,points:CurvePoints,kind:'concat_curves'|'concat_adjust'='concat_curves'):NativeNestedProject{
 if(points.length>8||points.some(p=>p.some(v=>!Number.isFinite(v)||v<0||v>1)))throw new Error('Некорректные точки кривой Concat')
 return mapNativeClipEffects(base,compositionId,trackId,clipId,effects=>{const effect=effects[index];if(!effect||(effect.kind!=='concat_curves'&&effect.kind!=='concat_adjust')||effect.kind!==kind)throw new Error('Кривая не найдена');return effects.map((e,i)=>i===index?{...effect,parameters:{...effect.parameters,[channel]:points}}:e)})
}
