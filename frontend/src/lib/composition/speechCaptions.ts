import type { LocalSpeechTranscript } from '$lib/api'
import type { CaptionStyle, CaptionTrack } from './types'

export const speechCaptionStyle = {
  wrapWidth: 0.84,
  margin: 0.08,
  size: 54,
  align: 'center',
  anchor: 'bottom',
  background: false,
  outline: 3.24,
} satisfies Partial<CaptionStyle>

export function speechCaptionTrack(transcript:LocalSpeechTranscript):CaptionTrack {
  if(!/^[a-f0-9]{64}$/.test(transcript.modelHash) || !/^[a-z-]{2,16}$/.test(transcript.language) || !Number.isSafeInteger(transcript.durationUs) || transcript.durationUs<=0 || transcript.durationUs>600000000 || transcript.segments.length>512)throw new Error('Некорректный результат распознавания')
  let previous=0
  const clips=transcript.segments.map((segment,index)=>{
    if(!Number.isSafeInteger(segment.startUs)||!Number.isSafeInteger(segment.endUs)||segment.startUs<previous||segment.endUs<=segment.startUs||segment.endUs>transcript.durationUs||!segment.text.trim()||segment.text.length>4096)throw new Error('Некорректное время или текст распознанного субтитра')
    previous=segment.endUs
    return{id:`speech-cue-${index}`,timelineStartTick:segment.startUs,timelineEndTick:segment.endUs,text:segment.text}
  })
  return{id:'speech-captions',name:`Распознанная речь (${transcript.language})`,format:'Subtitle',language:transcript.language,syncLock:true,hidden:false,locked:false,style:speechCaptionStyle,clips}
}
