import {rewindowNativeClipSourceEffects} from './concatSourceWindow'
import type {WireGraphicClip} from './types'

/** Concat layers rewindow package keys, but native parameter keys use absolute timeline time. */
export function windowNativeTreatment<T extends object>(clip:T,from:number,to:number,split?:'head'|'tail'):T|undefined{
 if(!('graphic' in clip)||!('concatTreatment' in clip)||!clip.concatTreatment)return undefined
 const layer=clip as unknown as WireGraphicClip,treatment=layer.concatTreatment!
 const offset=layer.animationOffsetUs??0
 const windowed=rewindowNativeClipSourceEffects(layer,treatment.durationTicks,offset+from,offset+to)
 return {...windowed,animationOffsetUs:0,concatTreatment:{...treatment,durationTicks:to-from,...(split==='head'?{fadeOut:0}:split==='tail'?{fadeIn:0}:{})}} as unknown as T
}
