import { expect,it } from 'vitest'
import { speechCaptionTrack } from './speechCaptions'
const transcript={language:'ru',modelHash:'a'.repeat(64),sourceStartUs:2000000,durationUs:3000000,segments:[{startUs:120000,endUs:930001,text:'Пример речи'},{startUs:930001,endUs:1800000,text:'Вторая строка'}]}
it('keeps source-relative speech times, language and text without a frame-grid roundtrip',()=>{
  const track=speechCaptionTrack(transcript)
  expect(track.language).toBe('ru');expect(track.clips.map(cue=>[cue.timelineStartTick,cue.timelineEndTick,cue.text])).toEqual([[120000,930001,'Пример речи'],[930001,1800000,'Вторая строка']])
})
it('refuses malformed/overlapping speech output rather than authoring invalid captions',()=>{
  expect(()=>speechCaptionTrack({...transcript,modelHash:'invalid'})).toThrow('Некорректный')
  expect(()=>speechCaptionTrack({...transcript,segments:[transcript.segments[1]!,transcript.segments[0]!]})).toThrow('Некорректное')
  expect(()=>speechCaptionTrack({...transcript,segments:[{startUs:0,endUs:4000000,text:'outside'}]})).toThrow('Некорректное')
})
