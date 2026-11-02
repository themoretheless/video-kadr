import type {NativeNestedProject,NestedCompositionProjectDto} from '../api'
import type {Job} from '../types'
import type {PendingVideoSmartStroke} from './pendingVideoSmart'
import {validateVideoRegion} from '../video-region'
export interface SmartVideoBase {documentHash:string;strokeCount:number;custom:boolean;hadCorrection:boolean;sourceIds:string[]}
function canonical(value:unknown):string{
 if(Array.isArray(value))return `[${value.map(canonical).join(',')}]`
 if(value&&typeof value==='object')return `{${Object.entries(value).filter(([,v])=>v!==undefined).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${JSON.stringify(k)}:${canonical(v)}`).join(',')}}`
 return JSON.stringify(value)??'null'
}
export async function smartDocumentHash(value:unknown):Promise<string>{const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(canonical(value)));return Array.from(new Uint8Array(digest),b=>b.toString(16).padStart(2,'0')).join('')}
export async function captureSmartVideoBase(project:NativeNestedProject,compositionId:string,clipId:string):Promise<SmartVideoBase>{
 const node=project.compositions[compositionId],binding=node?.content.tracks.flatMap(t=>t.kind==='video'?t.clips:[]).find(c=>c.id===clipId)?.concatCutout
 if(!binding)throw new Error('Нет исходных настроек умной видеокисти')
 return {documentHash:await smartDocumentHash(project),strokeCount:binding.correction?.strokes.length??0,custom:binding.correction?.custom??false,hadCorrection:binding.correction!==undefined,sourceIds:Object.keys(node!.content.sources).sort()}
}
const object=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{}
/** Prove this is exactly the native append, by job hashes and reversal to the captured document. */
export async function verifiedSmartVideoAppend(intent:PendingVideoSmartStroke,saved:NestedCompositionProjectDto,job:Job):Promise<boolean>{
 try{
  const base=intent.base;if(!base||saved.id!==intent.projectId||saved.revision!==intent.started.expectedRevision+1||job.id!==intent.started.jobId||job.status!=='done')return false
  const node=saved.document.compositions[intent.compositionId],clip=node?.content.tracks.flatMap(t=>t.kind==='video'?t.clips:[]).find(c=>c.id===intent.clipId),binding=clip?.concatCutout,correction=binding?.correction,index=base.strokeCount,stroke=correction?.strokes[index]
  if(!binding||clip?.sourceId!==intent.started.sourceId||binding.sourceHash!==intent.sourceHash||!correction?.custom||correction.strokes.length!==index+1||!stroke||stroke.tool!==intent.tool||stroke.size!==intent.size||stroke.region!==undefined||typeof stroke.at!=='number'||Math.round(stroke.at*1_000_000)!==intent.started.sourceUs||canonical(stroke.points)!==canonical(intent.points))return false
  const matches=binding.regionTracks?.filter(t=>t.stroke===index)??[];if(matches.length!==1)return false
  const region=matches[0]!.snapshot;validateVideoRegion(region)
  const metadata=object(job.result),tracking=object(metadata.tracking),movie=object(metadata.regionVideo),seed=object(metadata.seedRegion),rate=object(metadata.frameRate)
  if(region.sourceId!==intent.started.sourceId||region.sourceHash!==intent.sourceHash||metadata.sourceHash!==intent.sourceHash||region.seedFrame!==intent.started.seedFrame||metadata.seedFrame!==region.seedFrame||region.frames!==intent.started.frames||metadata.frames!==region.frames||canonical(region.frameTimesUs)!==canonical(intent.started.frameTimesUs)||canonical(region.frameTimesUs)!==canonical(metadata.frameTimesUs)||region.width!==metadata.width||region.height!==metadata.height||canonical(region.points)!==canonical(intent.points)||region.trackedFrames!==tracking.trackedFrames||region.forwardLostAt!==tracking.forwardLostAt||region.backwardLostAt!==tracking.backwardLostAt||region.maskHash!==tracking.outputHash||region.maskFileHash!==movie.fileHash||region.encoderHash!==tracking.encoderHash||region.decoderHash!==tracking.decoderHash||region.runtimeHash!==tracking.runtimeHash||region.promptHash!==seed.promptHash||rate.numerator!==region.frameRate.numerator||rate.denominator!==region.frameRate.denominator||region.frameRate.numerator!==intent.started.frameRate.numerator||region.frameRate.denominator!==intent.started.frameRate.denominator)return false
  const source=node!.content.sources[region.maskSourceId];if(source?.kind!=='video'||source.hasAudio||source.width!==256||source.height!==256)return false
  const inverse=JSON.parse(JSON.stringify(saved.document)) as NativeNestedProject,oldNode=inverse.compositions[intent.compositionId]!,oldClip=oldNode.content.tracks.flatMap(t=>t.kind==='video'?t.clips:[]).find(c=>c.id===intent.clipId)!
  const previous=JSON.parse(JSON.stringify(binding)) as Record<string,unknown>
  if(base.hadCorrection)previous.correction={...correction,custom:base.custom,strokes:correction.strokes.slice(0,index)};else delete previous.correction
  const tracks=binding.regionTracks!.filter(t=>t.stroke!==index);if(tracks.length)previous.regionTracks=tracks;else delete previous.regionTracks
  ;(oldClip as unknown as {concatCutout:unknown}).concatCutout=previous
  if(!base.sourceIds.includes(region.maskSourceId))delete (oldNode.content.sources as Record<string,unknown>)[region.maskSourceId]
  return await smartDocumentHash(inverse)===base.documentHash
 }catch{return false}
}
