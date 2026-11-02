import {speedRampTimelineDurationTicks} from './speedRamp'
import {renderNestedPreview,solveColorMatch,type NativeNestedProject,type NativeColorMatch} from '$lib/api'

/** Render the current shot independently of other tracks and with only the selected Lumetri disabled. */
export function colorMatchProbe(project:NativeNestedProject,compositionId:string,trackId:string,clipId:string,effectIndex:number):NativeNestedProject {
 const composition=project.compositions[compositionId]
 const track=composition?.content.tracks.find(t=>t.id===trackId)
 if(!composition||track?.kind!=='video')throw new Error('Видеодорожка не найдена')
 const clip=track.clips.find(c=>c.id===clipId)
 if(!clip||clip.effects[effectIndex]?.kind!=='lumetri')throw new Error('Эффект Lumetri не найден')
 return {...project,root:compositionId,compositions:{...project.compositions,[compositionId]:{
  ...composition,instances:[],content:{...composition.content,
   canvas:{...composition.content.canvas,backgroundMode:'color',background:{red:0,green:0,blue:0,alpha:0}},
   tracks:[{...track,hidden:false,clips:[{...clip,effects:clip.effects.map((e,i)=>i===effectIndex&&e.kind==='lumetri'?{...e,parameters:{...e.parameters,enabled:false}}:e)}]}],
  },
 }}}
}
export async function matchNestedShot(project:NativeNestedProject,compositionId:string,trackId:string,clipId:string,effectIndex:number,currentUs:number,referenceUs:number,renderedOutputs:Readonly<Record<string,string>>,token:string,skinProtect:boolean,signal?:AbortSignal):Promise<NativeColorMatch> {
 const probe=colorMatchProbe(project,compositionId,trackId,clipId,effectIndex)
 const track=project.compositions[compositionId]!.content.tracks.find(t=>t.id===trackId)!
 if(track.kind!=='video')throw new Error('Видеодорожка не найдена')
 const clip=track.clips.find(c=>c.id===clipId)!
 const placement=clip.placement
 const duration=placement.timelineDurationTick ?? (placement.speedRamp?speedRampTimelineDurationTicks(placement.sourceOutTick-placement.sourceInTick,placement.speed,placement.speedRamp):Math.round((placement.sourceOutTick-placement.sourceInTick)/placement.speed))
 const start=placement.timelineStartTick,end=start+duration
 if(!Number.isSafeInteger(currentUs)||!Number.isSafeInteger(referenceUs)||referenceUs<0||!Number.isSafeInteger(end)||duration<1)throw new Error('Некорректное время Color Match')
 if(referenceUs>=start&&referenceUs<end)throw new Error('Выберите эталонный кадр из другого клипа')
 const sampleUs=Math.min(Math.max(currentUs,start),end-1)
 const effect=clip.effects[effectIndex]!
 if(effect.kind!=='lumetri')throw new Error('Эффект Lumetri не найден')
 const [current,reference]=await Promise.all([
  renderNestedPreview(probe,sampleUs,{},token,signal,compositionId),
  renderNestedPreview(project,referenceUs,renderedOutputs,token,signal,compositionId),
 ])
 return solveColorMatch({width:current.width,height:current.height,rgba:Array.from(current.rgba)},{width:reference.width,height:reference.height,rgba:Array.from(reference.rgba)},effect.parameters,skinProtect,signal,project.luts ?? [])
}
