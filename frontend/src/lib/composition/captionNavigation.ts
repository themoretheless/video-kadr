import {compareCaptionTimes,type CaptionTimestamp} from './captionTime'
import type {CaptionCue,Composition} from './types'

/** Source navigation skips hidden tracks and uses strict start-time boundaries. */
export function captionNavigationTarget(composition:Composition,time:CaptionTimestamp,forward:boolean):CaptionCue {
 const cues=composition.captionTracks?.filter(track=>!track.hidden).flatMap(track=>track.clips).sort((a,b)=>compareCaptionTimes(a.timelineStartTick,b.timelineStartTick)||(a.id<b.id?-1:a.id>b.id?1:0))??[]
 const cue=forward?cues.find(cue=>compareCaptionTimes(cue.timelineStartTick,time)>0):cues.findLast(cue=>compareCaptionTimes(cue.timelineStartTick,time)<0)
 if(!cue)throw new Error('Больше нет субтитров')
 return cue
}
