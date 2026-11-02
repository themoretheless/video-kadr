import {writeFileSync} from 'node:fs'
import {expect,it} from 'vitest'
import type {Composition,AudioClip,VideoClip} from './types'
import type {NativeNestedProject} from '$lib/api'
import {createComposition,splitClip,trimClip} from './commands'
import {buildCompositionWireDocument} from './payload'
import {splitNativeClips} from './nativeClipSplit'
import {trimNativeClipEnds,trimNativeClipStarts} from './nativeClipTrim'
function fixture(kind:'audio'|'video'):Composition{
 const timing={id:'clip',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,audioStream:2,dspEffects:[{id:'concat.pitch',params:{semitones:0},concatSourceKeys:{semitones:[{at:0,value:-12,ease:[0,0,1,1] as const},{at:1,value:12,ease:[0,0,1,1] as const}]}}]}
 const clip=kind==='audio'?{...timing,kind,gain:1}:{...timing,kind,sourceAudioEnabled:true,audioGain:1,opacity:1,transform:{x:0,y:0,width:16,height:16,fit:'contain' as const}}
 return {...createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video',durationTicks:1000000,width:16,height:16,hasAudio:true,audioChannels:1}},tracks:[kind==='audio'?{id:'track',name:'Audio',kind,muted:false,locked:false,clips:[clip as AudioClip]}:{id:'track',name:'Video',kind,muted:false,hidden:false,locked:false,transitions:[],clips:[clip as VideoClip]}]}
}
function native(document:Composition):NativeNestedProject{return {schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(document),instances:[]}}}}
it('rewindows inactive DSP keys on ordinary and native audio/video split and trim without changing static params',()=>{
 const captured:{name:string;project:NativeNestedProject}[]=[]
 for(const kind of ['audio','video']as const){
  const input=fixture(kind),before=JSON.stringify(input),project=native(input)
  const ordinarySplit=native(splitClip(input,'clip',500000,'tail'))
  const nativeSplit=splitNativeClips(project,'root',['clip'],500000,()=> 'tail')
  const ordinaryTrim=native(trimClip(input,'clip',250000,750000))
  const nativeTrim=trimNativeClipEnds(trimNativeClipStarts(project,'root',['clip'],250000),'root',['clip'],-250000)
  for(const [name,output]of [['ordinary-split',ordinarySplit],['native-split',nativeSplit],['ordinary-trim',ordinaryTrim],['native-trim',nativeTrim]]as const){
   const clips=output.compositions.root!.content.tracks[0]!.clips as readonly (import('./types').WireAudioClip|import('./types').WireVideoClip)[]
   const values=clips.map(clip=>clip.dspEffects![0]!.concatSourceKeys!.semitones!.map(key=>key.value))
   expect(values).toEqual(name.endsWith('split')?[[-12,0],[0,12]]:[[-6,6]])
   for(const clip of clips){expect(clip.dspEffects![0]!.params).toEqual({semitones:0});expect(clip.dspEffects![0]!.animation).toBeUndefined();expect(clip.audioStream).toBe(2)}
   captured.push({name:`${kind}-${name}`,project:output})
  }
  expect(JSON.stringify(input)).toBe(before)
 }
 if(process.env.AUDIO_SOURCE_KEY_WINDOWS_OUTPUT)writeFileSync(process.env.AUDIO_SOURCE_KEY_WINDOWS_OUTPUT,JSON.stringify(captured))
})
