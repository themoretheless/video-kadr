import {validateVideoRegion} from './video-region'
import type {VideoFrameCorrection,VideoCorrectionRecipe} from './api'
import type {SamVideoRegionAttachment} from './composition/types'
export function canonicalVideoRecipe(edits:VideoFrameCorrection[]):string {
 return JSON.stringify([...edits].sort((a,b)=>a.frame-b.frame).map(edit=>({frame:edit.frame,correction:{custom:edit.correction.custom,strokes:edit.correction.strokes.map(s=>({tool:s.tool,size:s.size,points:s.points,at:null,...(s.region?{region:{sourceId:s.region.sourceId,maskSourceId:s.region.maskSourceId,sourceHash:s.region.sourceHash,maskHash:s.region.maskHash,maskFileHash:s.region.maskFileHash,encoderHash:s.region.encoderHash,decoderHash:s.region.decoderHash,runtimeHash:s.region.runtimeHash,promptHash:s.region.promptHash,points:s.region.points}}:{})})),feather:edit.correction.feather}})))
}
export function validateVideoRecipe(value:VideoCorrectionRecipe,frames:number,regions:readonly SamVideoRegionAttachment[]=value.projectOrigin?.snapshot?.trackedRegions??[]):void {
 if(!Number.isSafeInteger(frames)||frames<1||frames>360000||typeof value?.baseJobId!=='string'||!value.baseJobId||typeof value.maskHash!=='string'||!/^[a-f0-9]{64}$/.test(value.maskHash)||!Array.isArray(value.corrections)||value.corrections.length>1024)throw new Error('Неверный сохранённый рецепт видео')
 if(!Array.isArray(regions)||regions.length>1024)throw new Error('Неверный набор видеорегионов')
 const inventory=new Map<string,SamVideoRegionAttachment>();let prompts=0
 for(const region of regions){validateVideoRegion(region);prompts+=region.points.length;if(region.frames!==frames||inventory.has(region.maskSourceId))throw new Error('Неверная привязка видеорегиона');inventory.set(region.maskSourceId,region)}
 if(prompts>65536||65536*frames*regions.length>64*1024*1024*1024)throw new Error('Достигнут предел видеорегионов')
 let points=0,strokes=0;const seen=new Set<number>()
 for(const edit of value.corrections){
  if(!Number.isSafeInteger(edit.frame)||edit.frame<0||edit.frame>=frames||seen.has(edit.frame))throw new Error('Неверный номер кадра правки');seen.add(edit.frame)
  const c=edit.correction
  if(typeof c?.custom!=='boolean'||!Number.isFinite(c.feather)||c.feather<0||c.feather>0.1||!Array.isArray(c.strokes))throw new Error('Неверная правка кадра')
  strokes+=c.strokes.length
  for(const s of c.strokes){if(!['brush','eraser','smartBrush','smartEraser'].includes(s.tool)||!Number.isFinite(s.size)||s.size<0.005||s.size>0.5||s.at!=null||!Array.isArray(s.points)||!s.points.length||s.points.some(p=>!Array.isArray(p)||p.length!==2||p.some(v=>!Number.isFinite(v)||v<-.5||v>1.5)))throw new Error('Неверный штрих кадра');points+=s.points.length
   if(s.region!==undefined){const r=s.region,tracked=r&&inventory.get(r.maskSourceId)
    if(!r||!tracked||!['smartBrush','smartEraser'].includes(s.tool)||typeof r.maskHash!=='string'||!/^[a-f0-9]{64}$/.test(r.maskHash)||JSON.stringify(r.points)!==JSON.stringify(s.points)||['sourceId','maskSourceId','sourceHash','maskFileHash','encoderHash','decoderHash','runtimeHash','promptHash','points'].some(key=>JSON.stringify(r[key as keyof typeof r])!==JSON.stringify(tracked[key as keyof typeof tracked]))||Object.keys(r).some(key=>!['sourceId','maskSourceId','sourceHash','maskHash','maskFileHash','encoderHash','decoderHash','runtimeHash','promptHash','points'].includes(key)))throw new Error('Неверная привязка штриха к видеорегиону')
   }
  }
 }
 if(points>65536||strokes>1024)throw new Error('Достигнут предел штрихов видео')
}

export function validateVideoAttachment(value:unknown):asserts value is import('./composition/types').VideoMaskAttachment {
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Invalid video-mask attachment')
 const v=value as Record<string,unknown>;if(typeof v.sourceId!=='string'||!v.sourceId||typeof v.maskSourceId!=='string'||!v.maskSourceId||v.sourceId===v.maskSourceId)throw new Error('Invalid video-mask sources')
 for(const key of ['sourceHash','maskHash','maskFileHash'])if(typeof v[key]!=='string'||!/^[a-f0-9]{64}$/.test(v[key] as string))throw new Error('Invalid video-mask digest')
 for(const key of ['width','height','maskWidth','maskHeight','frames'])if(typeof v[key]!=='number'||!Number.isSafeInteger(v[key])||(v[key] as number)<=0)throw new Error('Invalid video-mask dimensions')
 const width=v.width as number,height=v.height as number,frames=v.frames as number;if(width*height>64*1024*1024||frames>360000||width*height*4*frames>64*1024*1024*1024)throw new Error('Video-mask source budget')
 const scale=512/Math.max(width,height),edge=(n:number)=>Math.max(64,Math.round(n*scale/16)*16)
 if(v.maskWidth!==edge(width)||v.maskHeight!==edge(height)||(v.maskWidth as number)*(v.maskHeight as number)*frames>64*1024*1024*1024)throw new Error('Video-mask layout mismatch')
 const rate=v.frameRate as {numerator?:unknown;denominator?:unknown}|null;if(!rate||typeof rate!=='object'||Array.isArray(rate)||![rate.numerator,rate.denominator].every(n=>typeof n==='number'&&Number.isInteger(n)&&n>0&&n<=4294967295))throw new Error('Invalid video-mask frame clock')
 validateVideoRecipe({baseJobId:v.sourceId,maskHash:v.maskHash as string,corrections:v.corrections as VideoFrameCorrection[]},frames,v.trackedRegions as SamVideoRegionAttachment[]|undefined)
 if(v.trackedRegions!==undefined){
  if(!Array.isArray(v.trackedRegions)||v.trackedRegions.length>1024)throw new Error('Invalid retained video regions')
  const seen=new Set<string>();let points=0
  for(const region of v.trackedRegions){validateVideoRegion(region);points+=region.points.length
   if(region.sourceHash!==v.sourceHash||region.width!==width||region.height!==height||region.frames!==frames||region.frameRate.numerator!==rate.numerator||region.frameRate.denominator!==rate.denominator||region.maskSourceId===v.maskSourceId||region.maskSourceId===v.sourceId||seen.has(region.maskSourceId))throw new Error('Retained region belongs to another video');seen.add(region.maskSourceId)
  }
  if(points>65536||65536*frames*v.trackedRegions.length>64*1024*1024*1024)throw new Error('Retained video region budget')
 }

 const keys=(object:object,allowed:string[])=>{if(Object.keys(object).some(key=>!allowed.includes(key)))throw new Error('Unknown video-mask field')}
 keys(v,['sourceId','maskSourceId','sourceHash','maskHash','maskFileHash','width','height','maskWidth','maskHeight','frames','frameRate','corrections','trackedRegions']);keys(rate,['numerator','denominator'])
 for(const edit of v.corrections as VideoFrameCorrection[]){keys(edit,['frame','correction']);keys(edit.correction,['custom','strokes','feather']);for(const stroke of edit.correction.strokes)keys(stroke,['tool','size','points','at','region'])}

}

function videoRangePlan(value:VideoCorrectionRecipe,frames:number,sourceFrame:number,start:number,end:number,regions?:readonly SamVideoRegionAttachment[]){
 validateVideoRecipe(value,frames,regions)
 if(![sourceFrame,start,end].every(Number.isSafeInteger)||sourceFrame<0||sourceFrame>=frames||start<0||end>frames||start>=end)throw new Error('Неверный диапазон кадров')
 const source=value.corrections.find(edit=>edit.frame===sourceFrame)?.correction??{custom:false,strokes:[],feather:0}
 const outside=value.corrections.filter(edit=>edit.frame<start||edit.frame>=end),repeat=source.custom||source.strokes.length||source.feather?end-start:0
 const strokes=outside.reduce((n,edit)=>n+edit.correction.strokes.length,0)+repeat*source.strokes.length
 const points=(c:typeof source)=>c.strokes.reduce((n,s)=>n+s.points.length,0)
 if(outside.length+repeat>1024||strokes>1024||outside.reduce((n,e)=>n+points(e.correction),0)+repeat*points(source)>65536)throw new Error('Достигнут предел штрихов видео')
 return {source,outside,repeat}
}
/** Replace a half-open ordinal range; plain callers cannot reuse learned plane hashes. */
export function copyVideoCorrectionRange(value:VideoCorrectionRecipe,frames:number,sourceFrame:number,start:number,end:number,regions?:readonly SamVideoRegionAttachment[]):VideoFrameCorrection[]{
 const {source,outside,repeat}=videoRangePlan(value,frames,sourceFrame,start,end,regions)
 if(source.strokes.some(s=>s.region)&&(start!==sourceFrame||end!==sourceFrame+1))throw new Error('Для диапазона нужны отдельные привязки региона к каждому кадру')
 const result=[...outside]
 for(let frame=start;repeat&&frame<end;frame++)result.push({frame,correction:JSON.parse(JSON.stringify(source))})
 result.sort((a,b)=>a.frame-b.frame);return result
}

/** Resolve each distinct region/ordinal privately before returning one complete edit. */
export async function copyVideoRegionCorrectionRange(value:VideoCorrectionRecipe,frames:number,sourceFrame:number,start:number,end:number,regions:readonly SamVideoRegionAttachment[],resolve:(regionId:string,frame:number,signal:AbortSignal)=>Promise<string>,signal:AbortSignal,onprogress?:(done:number,total:number)=>void):Promise<VideoFrameCorrection[]>{
 signal.throwIfAborted()
 const {source,outside,repeat}=videoRangePlan(value,frames,sourceFrame,start,end,regions)
 // Own a bounded snapshot before the first await, including unchanged outside edits.
 const template=JSON.parse(JSON.stringify(source)) as typeof source,result=JSON.parse(JSON.stringify(outside)) as VideoFrameCorrection[],inventory=JSON.parse(JSON.stringify(regions)) as SamVideoRegionAttachment[],baseJobId=value.baseJobId,maskHash=value.maskHash
 const ids=[...new Set(template.strokes.flatMap(s=>s.region?[s.region.maskSourceId]:[]))],total=repeat*ids.length;let done=0
 onprogress?.(done,total)
 for(let frame=start;repeat&&frame<end;frame++){
  signal.throwIfAborted()
  const hashes=new Map<string,string>()
  for(const id of ids){const hash=await resolve(id,frame,signal);signal.throwIfAborted();if(!/^[a-f0-9]{64}$/.test(hash))throw new Error('Неверный хеш кадра области');hashes.set(id,hash);onprogress?.(++done,total)}
  const correction=JSON.parse(JSON.stringify(template)) as typeof source
  result.push({frame,correction:{...correction,strokes:correction.strokes.map(stroke=>stroke.region?{...stroke,region:{...stroke.region,maskHash:hashes.get(stroke.region.maskSourceId)!}}:stroke)}})
 }
 signal.throwIfAborted();result.sort((a,b)=>a.frame-b.frame)
 validateVideoRecipe({baseJobId,maskHash,corrections:result},frames,inventory);return result
}
