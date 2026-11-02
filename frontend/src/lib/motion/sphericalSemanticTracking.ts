import type {SamVideoRegionAttachment,WireAnimatableValue} from '$lib/composition/types'
import {validateVideoRegion,videoRegionOrdinal} from '$lib/video-region'

/** The persisted SAM movie has a CFR decoder-ordinal clock even when its source has VFR PTS. */
export function sphericalSemanticSamplePlan(region:SamVideoRegionAttachment,sourceInSeconds:number,duration:number,allowLost=false):{times:number[];ordinals:number[]} {
 validateVideoRegion(region)
 if(!Number.isFinite(sourceInSeconds)||sourceInSeconds<0||!Number.isFinite(duration)||duration<.2)throw new Error('Некорректный диапазон семантического трекинга')
 const times:number[]=[],ordinals:number[]=[]
 const end=Math.min(duration,60)
 const rate=allowLost?Math.max(1,3/end):5
 for(let i=0;i<Math.min(allowLost?60:300,Math.ceil(end*rate));i++){
  const local=i/rate,sourceUs=Math.round((sourceInSeconds+local)*1e6),ordinal=videoRegionOrdinal(region,sourceUs)
  if(!allowLost&&(region.forwardLostAt!==null&&ordinal>=region.forwardLostAt||region.backwardLostAt!==null&&ordinal<=region.backwardLostAt))throw new Error('Область SAM потеряна внутри диапазона; уточните область или сократите клип')
  if(ordinals.at(-1)===ordinal)continue
  times.push(local);ordinals.push(ordinal)
 }
 if(times.length<2)throw new Error('Нужно минимум два исходных кадра SAM')
 return {times,ordinals}
}

/** Native measurements become ordinary microsecond camera channels. */
export function sphericalCentresCameraKeys(centres:readonly {pan:number;tilt:number;fov?:number}[],times:readonly number[]):{pan:WireAnimatableValue;tilt:WireAnimatableValue;fov?:WireAnimatableValue}{
 if(centres.length<2||centres.length>300||times.length!==centres.length||centres.some(c=>!Number.isFinite(c.pan)||c.pan< -180||c.pan>180||!Number.isFinite(c.tilt)||c.tilt< -90||c.tilt>90)||times.some((t,i)=>!Number.isFinite(t)||t<0||!Number.isSafeInteger(Math.round(t*1e6))||(i>0&&Math.round(t*1e6)<=Math.round(times[i-1]!*1e6))))throw new Error('Некорректные измерения камеры SAM')
 const hasFov=centres.some(c=>c.fov!==undefined)
 if(hasFov&&centres.some(c=>c.fov===undefined||!Number.isFinite(c.fov)||c.fov<10||c.fov>179))throw new Error('Некорректный ряд FOV камеры SAM')
 const positions=centres.map(c=>({...c}))
 for(let i=1;i<positions.length;i++){while(positions[i]!.pan-positions[i-1]!.pan>180)positions[i]!.pan-=360;while(positions[i]!.pan-positions[i-1]!.pan< -180)positions[i]!.pan+=360}
 const channel=(key:'pan'|'tilt'|'fov'):WireAnimatableValue=>({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:positions.map((p,i)=>({tick:Math.round(times[i]!*1e6),value:p[key]!}))}})
 return {pan:channel('pan'),tilt:channel('tilt'),...(hasFov?{fov:channel('fov')}:{})}
}

/** Reidentification reports only real visible measurements. Gaps are not masks. */
export function sphericalIdentityMeasurements(result:{centres:readonly {pan:number;tilt:number;fov?:number}[];sampleIndices?:readonly number[];states?:readonly string[]},times:readonly number[]):{centres:readonly {pan:number;tilt:number;fov?:number}[];times:number[];gaps:number;recovered:number}{
 const indices=result.sampleIndices,states=result.states
 if(!indices||!states||states.length!==times.length||indices.length!==result.centres.length||indices.length<2||indices.some((v,i)=>!Number.isSafeInteger(v)||v<0||v>=times.length||(i>0&&v<=indices[i-1]!))||states.some(v=>!['seed','tracked','reacquired','occluded','ambiguous'].includes(v)))throw new Error('Некорректный ряд повторной идентификации')
 const measured=new Set(indices)
 if(states.some((value,i)=>measured.has(i)!==['seed','tracked','reacquired'].includes(value)))throw new Error('Измерения не соответствуют видимости объекта')
 return {centres:result.centres,times:indices.map(i=>times[i]!),gaps:times.length-indices.length,recovered:states.filter(v=>v==='reacquired').length}
}
