import type { CaptionFileDocument } from '$lib/api'
import type { CaptionTrack } from './types'
import { exactCaptionTicks, type CaptionTimestamp } from './captionTime'

/** Preserve authored cue IDs/markup/voice/settings; IDs internal to the editor are not file IDs. */
export function captionFileDocument(track: CaptionTrack): CaptionFileDocument {
 const tick = (time: CaptionTimestamp) => {
  const value=exactCaptionTicks(time)
  if(value<0n||value>(1n<<63n)-1n)throw new Error('Время субтитра вне диапазона формата файла')
  return value.toString()
 }
 return {ticksPerSecond:'254016000000',blocks:[...(track.vttBlocks??[])],cues:track.clips.map(cue=>({startTick:tick(cue.timelineStartTick),endTick:tick(cue.timelineEndTick),text:cue.text,speaker:cue.speaker??null,id:cue.cueId??null,settings:cue.settings??''}))}
}
