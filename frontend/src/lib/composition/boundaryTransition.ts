import {defaultProceduralTransition} from './transitionCatalog'
import type {CompositionTransition,CompositionTransitionKind} from './types'
import {transitionSourceSpan} from './transitionTimeline'

export interface BoundaryTransitionOptions {readonly independent?:boolean;readonly startTick?:number|string;readonly reverse?:boolean;readonly kernel?:import('./transitionCatalog').ProceduralTransitionSpec}

export function boundaryTransition(id:string,fromClipId:string,toClipId:string,kind:CompositionTransitionKind,
 durationTicks:number,cutTick:number,existing?:CompositionTransition,options:BoundaryTransitionOptions={}):CompositionTransition {
 const duration=Math.max(1,Math.round(durationTicks))
 const independent=options.independent||existing?.timeline!==undefined||['concat_dissolve','concat_fade_black','concat_fade_white','procedural','film_dissolve','additive_dissolve','fade_white'].includes(kind)
 if(!independent)return {id,fromClipId,toClipId,kind,durationTicks:duration}
 const previous=existing?.timeline
 const previousDuration=previous?Number(transitionSourceSpan(previous)[1])/254016:undefined
 const timeline=previous?{
  startTick:options.startTick??previous.startTick,
  durationTick:Math.round(previousDuration!)===duration?previous.durationTick:duration,
 }:{startTick:options.startTick??cutTick-Math.floor(duration/2),durationTick:duration}
 return {id,fromClipId,toClipId,kind,durationTicks:0,timeline,...(kind==='procedural'?{kernel:options.kernel??existing?.kernel??defaultProceduralTransition()}:{}),...(options.reverse!==undefined?{reverse:options.reverse}:existing?.reverse!==undefined?{reverse:existing.reverse}:{})}
}
