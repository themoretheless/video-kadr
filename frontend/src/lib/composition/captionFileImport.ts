import type {PreparedCaptionFileImport} from '$lib/api'
import type {CaptionTrack,Composition} from './types'
import {normalizeComposition} from './validation'

export function readCaptionFileBase64(file: File): Promise<string> {
 return new Promise((resolve, reject) => {
  const reader = new FileReader()
  reader.onload = () => {
   const value = String(reader.result), comma = value.indexOf(',')
   if (comma < 0) reject(new Error('Не удалось прочитать файл'))
   else resolve(value.slice(comma + 1))
  }
  reader.onerror = () => reject(new Error('Не удалось прочитать файл'))
  reader.readAsDataURL(file)
 })
}

/** The native decoder owns parsing and frame snapping. */
export function preparedCaptionTrack(prepared: PreparedCaptionFileImport, allocate: () => string = () => crypto.randomUUID()): CaptionTrack {
 if(prepared.document.ticksPerSecond!=='254016000000')throw new Error('Некорректная шкала времени субтитров')
 return {id:`captions-${allocate()}`,name:prepared.name,format:prepared.format,language:prepared.language,hidden:!prepared.enabled,locked:prepared.locked,syncLock:prepared.syncLock,vttBlocks:[...(prepared.document.blocks??[])],style:{backgroundColor:'#'+prepared.backgroundColor.map(byte=>byte.toString(16).padStart(2,'0')).join('')},clips:prepared.document.cues.map(cue=>({id:`caption-${allocate()}`,cueId:cue.id??null,timelineStartTick:cue.startTick,timelineEndTick:cue.endTick,text:cue.text,speaker:cue.speaker??null,settings:cue.settings??''}))}
}

export function applyPreparedCaptionImport(before:Composition,prepared:PreparedCaptionFileImport,allocate:()=>string=()=>crypto.randomUUID()):{composition:Composition;trackId:string} {
 const track = preparedCaptionTrack(prepared, allocate)
 return {composition:normalizeComposition({...before,captionTracks:[track,...before.captionTracks??[]]}),trackId:track.id}
}
