import {exactCaptionDeltaTicks,type CaptionTimestamp} from './captionTime'
/** Native/source-clock interval, independent of adjacent clip cuts. */
export interface TransitionTimeline {readonly startTick:CaptionTimestamp;readonly durationTick:CaptionTimestamp}
export interface ExplicitTransitionWindow {readonly fromClipId:string|null;readonly toClipId:string|null;readonly timeline:TransitionTimeline}
const MIN=-(1n<<63n),MAX=(1n<<63n)-1n
const checked=(value:bigint)=>{if(value<MIN||value>MAX)throw new Error('Transition source clock overflow');return value}
export function transitionSourceSpan(timeline:TransitionTimeline):readonly [bigint,bigint] {
 const start=checked(exactCaptionDeltaTicks(timeline.startTick)),duration=checked(exactCaptionDeltaTicks(timeline.durationTick))
 if(duration<=0n)throw new Error('Transition duration must be positive')
 checked(start+duration);return [start,duration]
}
/** Original edit::insert: remap outgoing only; remove split single-sided fades. */
export function insertTransitionWindow<T extends ExplicitTransitionWindow>(transition:T,at:CaptionTimestamp,duration:CaptionTimestamp,splits:ReadonlyMap<string,string>):T|null {
 const [start,length]=transitionSourceSpan(transition.timeline),point=checked(exactCaptionDeltaTicks(at)),delta=checked(exactCaptionDeltaTicks(duration)),end=start+length
 if(delta<=0n)throw new Error('Insertion duration must be positive')
 const split=(transition.fromClipId!==null&&splits.has(transition.fromClipId))||(transition.toClipId!==null&&splits.has(transition.toClipId))
 if(start<point&&point<end&&split&&(transition.fromClipId===null||transition.toClipId===null))return null
 const shifted=start>=point||(end>point&&transition.fromClipId===null)
 const nextStart=shifted?checked(start+delta):start;checked(nextStart+length)
 return {...transition,fromClipId:start>=point&&transition.fromClipId!==null?(splits.get(transition.fromClipId)??transition.fromClipId):transition.fromClipId,timeline:{startTick:nextStart.toString(),durationTick:length.toString()}}
}
/** Half-open exact interval sampling; convert progress only after membership. */
export function transitionWindowProgress(timeline:TransitionTimeline,at:CaptionTimestamp):number|null {
 const [start,duration]=transitionSourceSpan(timeline),point=checked(exactCaptionDeltaTicks(at))
 if(point<start||point>=start+duration)return null
 return Number(point-start)/Number(duration)
}

/** Original split_track_at: keep exact timing, remap only outgoing after cut. */
export function splitTransitionWindow<T extends ExplicitTransitionWindow>(transition:T,at:CaptionTimestamp,clipId:string,rightClipId:string):T|null {
 const [start,duration]=transitionSourceSpan(transition.timeline),point=checked(exactCaptionDeltaTicks(at))
 const touches=transition.fromClipId===clipId||transition.toClipId===clipId
 if(touches&&start<point&&point<start+duration&&(transition.fromClipId===null||transition.toClipId===null))return null
 return start>=point&&transition.fromClipId===clipId?{...transition,fromClipId:rightClipId}:transition
}

/** Filmcraft add_transition: overlapping half-open windows replace shared sides.
 * Null equals null here too: overlapping fades sharing an absent side replace.
 */
export function transitionWindowReplaces(existing:ExplicitTransitionWindow,added:ExplicitTransitionWindow):boolean {
 const [a,ad]=transitionSourceSpan(existing.timeline),[b,bd]=transitionSourceSpan(added.timeline)
 return a<b+bd&&b<a+ad&&(existing.fromClipId===added.fromClipId||existing.toClipId===added.toClipId)
}
