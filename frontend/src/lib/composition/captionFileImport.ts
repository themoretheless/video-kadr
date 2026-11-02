import type {PreparedCaptionFileImport} from '$lib/api'
import type {CaptionTrack,Composition} from './types'
import {normalizeComposition} from './validation'

/** Native preparation owns parsing/frame snapping; this transaction allocates host IDs only. */
export function applyPreparedCaptionImport(before:Composition,prepared:PreparedCaptionFileImport,allocate:()=>string=()=>crypto.randomUUID()):{composition:Composition;trackId:string} {
 if(prepared.document.ticksPerSecond!=='254016000000')throw new Error('Некорректная шкала времени субтитров')
 const track:CaptionTrack={id:`captions-${allocate()}`,name:prepared.name,format:prepared.format,language:prepared.language,hidden:!prepared.enabled,locked:prepared.locked,syncLock:prepared.syncLock,vttBlocks:[...(prepared.document.blocks??[])],style:{backgroundColor:'#'+prepared.backgroundColor.map(byte=>byte.toString(16).padStart(2,'0')).join('')},clips:prepared.document.cues.map(cue=>({id:`caption-${allocate()}`,cueId:cue.id??null,timelineStartTick:cue.startTick,timelineEndTick:cue.endTick,text:cue.text,speaker:cue.speaker??null,settings:cue.settings??''}))}
 return {composition:normalizeComposition({...before,captionTracks:[track,...before.captionTracks??[]]}),trackId:track.id}
}
