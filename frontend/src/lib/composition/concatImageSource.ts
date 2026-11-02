import type {WireCompositionTrack,WireImageClip} from './types'
/** Imported Concat stills carry source timing or retained source picture controls. */
export function isConcatImageClip(clip:WireCompositionTrack['clips'][number]):clip is WireImageClip {
 return !('placement' in clip)&&'sourceId' in clip&&'durationTicks' in clip&&!!(clip.concatSourceTiming||clip.transform.concatPreparation||clip.transform.concatAnimation)
}
