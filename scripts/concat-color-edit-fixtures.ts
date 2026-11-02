import {curvePreviewProject,moveCurvePoint}from'../frontend/src/lib/composition/concatCurveAuthoring'
import {setWheel,keyWheel,wheelPreviewProject,type WheelEffect} from '../frontend/src/lib/composition/concatWheelAuthoring'
import {addClip,addTrack,createComposition,registerSource} from '../frontend/src/lib/composition/commands'
import {buildCompositionWireDocument} from '../frontend/src/lib/composition/payload'
import {splitNativeClips} from '../frontend/src/lib/composition/nativeClipSplit'
import {trimNativeClipStarts} from '../frontend/src/lib/composition/nativeClipTrim'
import {duplicateNativeClips} from '../frontend/src/lib/composition/nativeClipDuplicate'
import type {NativeNestedProject} from '../frontend/src/lib/api'
import type {WireAnimatableValue,WireVideoEffect,WireVideoClip} from '../frontend/src/lib/composition/types'
function curve(values:readonly number[]):WireAnimatableValue{return{mode:'keyframes',track:{timeBase:1000000,interpolation:'ease_in_out_cubic',extrapolation:{before:'hold',after:'hold'},keyframes:[-500000,0,2000000,4500000].map((tick,i)=>({tick,value:values[i]!}))}}}
function effect(kind:string):WireVideoEffect{
 if(kind==='concat_matcha'||kind==='concat_sage')return{kind,parameters:{amount:75}}
 if(kind==='concat_ivory')return{kind,parameters:{matte:75,warmth:80}}
 if(kind==='concat_milk')return{kind,parameters:{matte:75,cool:80}}
 if(kind==='concat_bronze')return{kind,parameters:{tone:75,contrast:80}}
 if(kind==='concat_cool_blue')return{kind,parameters:{chill:75,contrast:80}}
 if(kind==='concat_fresh')return{kind,parameters:{pop:75,brightness:80}}
 if(kind==='concat_vivid')return{kind,parameters:{colour:75,punch:80}}
 if(kind==='concat_soft_light')return{kind,parameters:{lift:75,warmth:80}}
 if(kind==='concat_moody')return{kind,parameters:{depth:75,warmth:80}}
 if(kind==='concat_rain')return{kind,parameters:{haze:75,chill:80}}
 if(kind==='concat_umber')return{kind,parameters:{warmth:75,soften:80}}
 if(kind==='concat_fair')return{kind,parameters:{brighten:75,even:80}}
 if(kind==='concat_peach')return{kind,parameters:{glow:75,softness:80}}
 if(kind==='concat_polaroid')return{kind,parameters:{fade:75,warmth:80}}
 if(kind==='concat_vintage')return{kind,parameters:{age:75,tint:80}}
 if(kind==='concat_cloudless_sky')return{kind,parameters:{sky:75,greens:80}}
 if(kind==='concat_lagoon')return{kind,parameters:{water:75,brightness:80}}
 if(kind==='concat_golden_hour')return{kind,parameters:{glow:75,warmth:4750}}
 if(kind==='concat_daydream')return{kind,parameters:{haze:75,warmth:80}}
 if(kind==='concat_clean')return{kind,parameters:{amount:75}}
 if(kind==='concat_classic_chrome')return{kind,parameters:{}}
 if(kind==='concat_cyberpunk')return{kind,parameters:{}}
 if(kind==='concat_delicious')return{kind,parameters:{}}
 if(kind==='concat_rosy')return{kind,parameters:{}}
 if(kind==='concat_sci_fi')return{kind,parameters:{}}
 if(kind==='concat_high_contrast_mono')return{kind,parameters:{}}
 if(kind==='concat_landscape_pop')return{kind,parameters:{}}
 if(kind==='concat_natural')return{kind,parameters:{}}
 if(kind==='concat_retro_70s')return{kind,parameters:{}}
 if(kind==='concat_silver')return{kind,parameters:{}}
 if(kind==='concat_tropical')return{kind,parameters:{}}
 if(kind==='concat_vaporwave')return{kind,parameters:{}}
 if(kind==='concat_winter_cool')return{kind,parameters:{}}
 if(kind==='concat_noir'||kind==='concat_dreamy')return{kind,parameters:{}}
 if(kind==='concat_cool')return{kind,parameters:{"temperature":9500}}
 if(kind==='concat_warm')return{kind,parameters:{"temperature":4750}}
 if(kind==='concat_teal_orange')return{kind,parameters:{"amount":75,"contrast":60,"skin":90}}
 if(kind==='concat_night')return{kind,parameters:{lift:75,clarity:80}}
 if(kind==='concat_spooky')return{kind,parameters:{dread:75}}
 if(kind==='concat_neon')return{kind,parameters:{glow:75,punch:80}}
 if(kind==='concat_pink_fuzz')return{kind,parameters:{haze:75,blush:80}}
 if(kind==='concat_autumn')return{kind,parameters:{}}
 if(kind==='concat_vhs')return{kind,parameters:{bleed:37,lines:80}}
 if(kind==='concat_falling_dust')return{kind,parameters:{dust:75,leak:80}}
 if(kind==='concat_film')return{kind,parameters:{grain:75,warmth:80}}
 if(kind==='concat_digicam')return{kind,parameters:{flash:75,noise:80}}
 if(kind==='concat_mono')return{kind,parameters:{colour:75,contrast:80,grain:75}}
 if(kind==='concat_midnight')return{kind,parameters:{depth:75,grain:80}}
 if(kind==='concat_marquee')return{kind,parameters:{lights:75,depth:80}}
 if(kind==='concat_retro_cartoon')return{kind,parameters:{levels:3.5,ink:75}}
 if(kind==='concat_super8')return{kind,parameters:{}}
 if(kind==='concat_bleach_bypass')return{kind,parameters:{}}
 if(kind==='concat_contrast_pop')return{kind,parameters:{contrast:1.75}}
 if(kind==='concat_sharpen')return{kind,parameters:{amount:1.7}}
 if(kind==='concat_vignette')return{kind,parameters:{strength:70}}
 if(kind==='concat_curves')return{kind,parameters:{luma:[[0,0],[0.25,0.18],[0.75,0.84],[1,1]],red:[[0.2,0.1],[0.8,0.9]],green:[[0,0.1],[0.5,0.8],[1,0.6]],blue:[[0,0.2],[1,0.8]]}}
 if(kind==='concat_color_wheels'||kind==='concat_adjust'){const original:WheelEffect={kind,parameters:kind==='concat_adjust'?{brightness:8,temperature:5000,tint:10,saturation:15,contrast:20,shadows:12,highlights:-8,whites:5,blacks:-4,sharpen:30,fade:8,vignette:25,luma:[[0,0],[0.5,0.6],[1,1]],blue:[[0,0.1],[1,0.9]]}:{},animation:{'lift.x':curve([0,0.2,-0.4,0.1]),'lift.y':curve([0,-0.3,0.4,0.1]),'lift.m':curve([0,0.1,-0.2,0.1]),'gamma.x':curve([0,0.2,-0.1,0.1]),'gamma.y':curve([0,-0.1,0.3,0.1]),'gamma.m':curve([0,-0.2,0.1,0.1]),'gain.x':curve([0,0.2,-0.1,0.1]),'gain.y':curve([0,0.1,-0.2,0.1]),'gain.m':curve([0,-0.1,0.2,0.1])}}
  const animation={...original.animation}
  for(const channel of ['lift.x','lift.m']as const){const value=animation[channel]!;if(value.mode!=='keyframes')throw new Error('curve');animation[channel]={...value,track:{...value.track,timeBase:1000,keyframes:value.track.keyframes.map(p=>({...p,tick:p.tick/1000}))}}}
  return keyWheel(setWheel({...original,animation},'lift',{x:0.45,m:-0.1},1250000),'lift',1250000)
 }
 if(kind==='concat_white_balance')return{kind,parameters:{temperature:6500,tint:0},animation:{temperature:curve([6500,2000,11000,4500]),tint:curve([0,-100,100,25])}}
 if(kind==='concat_contrast')return{kind,parameters:{amount:0},animation:curve([0,-40,60,10])}
 if(kind==='concat_exposure')return{kind,parameters:{stops:0},animation:curve([0,-1,1.5,0.25])}
 if(kind==='concat_saturation')return{kind,parameters:{amount:0},animation:curve([0,-30,50,10])}
 return{kind:'concat_vibrance',parameters:{amount:0.7,intensity:100},animation:{amount:curve([0.7,0.3,1.5,0.7]),intensity:curve([100,100,25,75])}}
}
export function makeFixture(){
 const cases=[]
 for(const kind of ['concat_contrast','concat_exposure','concat_saturation','concat_vibrance','concat_white_balance','concat_color_wheels','concat_curves','concat_adjust','concat_sharpen','concat_vignette','concat_contrast_pop','concat_bleach_bypass','concat_super8','concat_retro_cartoon','concat_marquee','concat_mono','concat_midnight','concat_film','concat_digicam','concat_falling_dust','concat_vhs','concat_autumn','concat_neon','concat_pink_fuzz','concat_night','concat_spooky','concat_cool','concat_warm','concat_teal_orange','concat_noir','concat_dreamy','concat_high_contrast_mono','concat_landscape_pop','concat_natural','concat_retro_70s','concat_silver','concat_tropical','concat_vaporwave','concat_winter_cool','concat_classic_chrome','concat_cyberpunk','concat_delicious','concat_rosy','concat_sci_fi','concat_clean','concat_matcha','concat_sage','concat_ivory','concat_milk','concat_bronze','concat_cool_blue','concat_fresh','concat_vivid','concat_soft_light','concat_moody','concat_rain','concat_umber','concat_fair','concat_peach','concat_polaroid','concat_vintage','concat_cloudless_sky','concat_lagoon','concat_golden_hour','concat_daydream','stack'])for(const mode of ['forward','reverse','freeze']as const){
  const speed=mode==='freeze'?1:2
  let d=registerSource(createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),{id:'source',kind:'video',durationTicks:10000000,width:16,height:16,hasAudio:false})
  d=addTrack(d,{id:'video',kind:'video',name:'Video',locked:false,hidden:false,muted:false,clips:[]})
  d=addClip(d,'video',{id:'clip',kind:'video',sourceId:'source',timelineStartTicks:1000000,sourceInTicks:1000000,sourceOutTicks:mode==='freeze'?5000000:9000000,speed,playbackMode:mode==='freeze'?{mode:'freeze',sourceTick:3250000}:mode==='reverse'?{mode:'reverse'}:{mode:'forward'},transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1})
  const content=buildCompositionWireDocument(d)
  const video=content.tracks[0]!
  if(video.kind!=='video')throw new Error('video')
  let original:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{...content,tracks:[{...video,clips:[{...video.clips[0]!,effects:kind==='stack'?['concat_contrast','concat_exposure','concat_saturation','concat_vibrance','concat_white_balance','concat_color_wheels','concat_curves','concat_adjust','concat_sharpen','concat_vignette','concat_contrast_pop','concat_bleach_bypass','concat_super8','concat_retro_cartoon','concat_marquee','concat_mono','concat_midnight','concat_film','concat_digicam','concat_falling_dust','concat_vhs','concat_autumn','concat_neon','concat_pink_fuzz','concat_night','concat_spooky','concat_cool','concat_warm','concat_teal_orange','concat_noir','concat_dreamy','concat_high_contrast_mono','concat_landscape_pop','concat_natural','concat_retro_70s','concat_silver','concat_tropical','concat_vaporwave','concat_winter_cool','concat_classic_chrome','concat_cyberpunk','concat_delicious','concat_rosy','concat_sci_fi','concat_clean','concat_matcha','concat_sage','concat_ivory','concat_milk','concat_bronze','concat_cool_blue','concat_fresh','concat_vivid','concat_soft_light','concat_moody','concat_rain','concat_umber','concat_fair','concat_peach','concat_polaroid','concat_vintage','concat_cloudless_sky','concat_lagoon','concat_golden_hour','concat_daydream'].map(effect):[effect(kind)]}]}]}}}}
  if(kind==='concat_color_wheels'||kind==='stack'){
   const clip=original.compositions.root!.content.tracks[0]!.clips[0]!as WireVideoClip
   const index=clip.effects!.findIndex(e=>e.kind==='concat_color_wheels')
   if(index<0)throw new Error('wheel preview target')
   original=wheelPreviewProject(original,'root','video','clip',index,'lift',{x:0.55,y:-0.1},1250000)
  }
  if(kind==='concat_curves'||kind==='stack'){
   const clip=original.compositions.root!.content.tracks[0]!.clips[0]!as WireVideoClip
   const index=clip.effects!.findIndex(e=>e.kind==='concat_curves'),selected=clip.effects![index]!
   if(selected.kind!=='concat_curves')throw new Error('curve preview target')
   original=curvePreviewProject(original,'root','video','clip',index,'blue',moveCurvePoint(selected.parameters.blue??[],1,0.9,0.65))
  }
  if(kind==='concat_adjust'||kind==='stack'){
   const clip=original.compositions.root!.content.tracks[0]!.clips[0]!as WireVideoClip
   const index=clip.effects!.findIndex(e=>e.kind==='concat_adjust')
   original=wheelPreviewProject(original,'root','video','clip',index,'lift',{x:0.55,y:-0.1},1250000,'concat_adjust')
   original=curvePreviewProject(original,'root','video','clip',index,'blue',[[0,0.15],[0.9,0.65]],'concat_adjust')
  }
  const before=JSON.stringify(original);let serial=0
  const split=splitNativeClips(original,'root',['clip'],2250000,()=>`tail-${serial++}`)
  const trimmed=trimNativeClipStarts(original,'root',['clip'],750000)
  const duplicated=duplicateNativeClips(original,'root',['clip'],()=>`copy-${serial++}`,6000000)
  const starts=(project:NativeNestedProject)=>project.compositions.root!.content.tracks.flatMap(track=>track.clips.map(clip=>'placement'in clip?clip.placement.timelineStartTick:clip.timelineStartTick))
  if(JSON.stringify(starts(split))!==JSON.stringify([1000000,2250000])||JSON.stringify(starts(trimmed))!==JSON.stringify([1750000])||JSON.stringify(starts(duplicated))!==JSON.stringify([1000000,6000000]))throw new Error('editor operation did not produce its requested structure')
  if(JSON.stringify(original)!==before)throw new Error('editor operation mutated original')
  // Original Concat grain reads absolute timeline time. Duplication changes phase;
  // compare the command output against a directly authored moved clip at that time.
  const shiftedReference:NativeNestedProject={...original,compositions:{...original.compositions,root:{...original.compositions.root!,content:{...original.compositions.root!.content,tracks:[{...video,clips:[{...original.compositions.root!.content.tracks[0]!.clips[0]!,placement:{...video.clips[0]!.placement,timelineStartTick:6000000}}]}]}}}}
  const times=[1000000,1250000,1750000,2249999,2250000,2250001,3250000,4750000]
  cases.push({name:`${kind}/${mode}`,kind,mode,original,edits:[{name:'split',project:split,times,offset:0},{name:'left-trim',project:trimmed,times:times.filter(t=>t>=1750000),offset:0},{name:'duplicate',project:duplicated,times,offset:5000000,...(kind==='concat_super8'||kind==='concat_mono'||kind==='concat_midnight'||kind==='concat_film'||kind==='concat_digicam'||kind==='concat_falling_dust'||kind==='concat_vhs'||kind==='stack'?{referenceProject:shiftedReference,referenceOffset:5000000}:{})}]})
 }
 return{schemaVersion:1,cases}
}
export const fixture=makeFixture()
