import {scalarClipboardProject} from './nativeScalarClipboardProject'
import {insertNativeConcatShape} from '../nativeShapeInsert'
export function effectScalarProject(){
 const empty=scalarClipboardProject();Object.assign(empty.compositions.root!.content,{tracks:[]})
 const project=insertNativeConcatShape(empty,'root',{clipId:'shape',trackId:'',newTrackId:'shapes',kind:'square',startUs:1000000,durationUs:2000000})
 const clip=project.compositions.root!.content.tracks[0]!.clips[0]!
 Object.assign(clip,{effects:[{kind:'concat_exposure',parameters:{stops:0},animation:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:-2},{tick:750,value:-1},{tick:1250,value:0}]}}},{kind:'concat_exposure',parameters:{stops:0},animation:{mode:'constant',value:0}}]})
 return project
}

export function effectColorScalarProject(){
 const project=effectScalarProject(),clip=project.compositions.root!.content.tracks[0]!.clips[0]!
 Object.assign(clip,{effects:[{kind:'color_tint',parameters:{},animation:{mode:'constant',value:100},colorAnimation:{whiteR:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:.25},{tick:750,value:.5},{tick:1250,value:.75}]}},whiteA:{mode:'constant',value:1}}},{kind:'concat_exposure',parameters:{stops:0},animation:{mode:'constant',value:0}}]})
 return project
}

export function effectLumetriScalarProject(){
 const project=effectScalarProject(),clip=project.compositions.root!.content.tracks[0]!.clips[0]!
 Object.assign(clip,{effects:[{kind:'lumetri',parameters:{enabled:true,curvesOn:true,wheelsOn:true,animation:{basic:{exposure:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:-2},{tick:750,value:-1},{tick:1250,value:0}]}}},creative:{fadedFilm:{mode:'constant',value:0}},grade:{shadowsX:{mode:'constant',value:0}},sharpen:{mode:'constant',value:0}}}}]})
 return project
}
