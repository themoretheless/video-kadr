import {videoRegionOrdinal} from '../video-region'
import type {NestedVideoSmartRegionJob} from '../api'
export interface PendingVideoSmartStroke {
 version:1|2;operationId?:string;base?:import('./smartVideoReceipt').SmartVideoBase;id:string;projectId:string;compositionId:string;clipId:string;sourceHash:string;timeUs:number;
 points:[number,number][];tool:'smartBrush'|'smartEraser';size:number;started:NestedVideoSmartRegionJob
}
export type PendingScope=Pick<PendingVideoSmartStroke,'projectId'|'compositionId'|'clipId'>
type Store=Pick<Storage,'getItem'|'setItem'|'removeItem'>
const MAX_BYTES=16*1024*1024
export async function pendingVideoSmartKey(token:string,scope:PendingScope):Promise<string>{
 const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(token))
 const owner=Array.from(new Uint8Array(digest),b=>b.toString(16).padStart(2,'0')).join('')
 return `video-kadr.pending-video-smart:${owner}:${encodeURIComponent(JSON.stringify([scope.projectId,scope.compositionId,scope.clipId]))}`
}
export interface PendingVideoSmartSubmission {
 version:3;id:string;projectId:string;compositionId:string;clipId:string;sourceId:string;sourceHash:string;expectedRevision:number;timeUs:number;
 points:[number,number][];tool:'smartBrush'|'smartEraser';size:number;base:import('./smartVideoReceipt').SmartVideoBase
}
export type PendingVideoSmartRecord=PendingVideoSmartStroke|PendingVideoSmartSubmission
const uuid=(value:unknown)=>typeof value==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value)
function validBase(b:import('./smartVideoReceipt').SmartVideoBase|undefined):boolean{
 return Boolean(b&&typeof b.documentHash==='string'&&/^[a-f0-9]{64}$/.test(b.documentHash)&&Number.isInteger(b.strokeCount)&&b.strokeCount>=0&&b.strokeCount<1024&&typeof b.custom==='boolean'&&typeof b.hadCorrection==='boolean'&&Array.isArray(b.sourceIds)&&b.sourceIds.length<=32&&new Set(b.sourceIds).size===b.sourceIds.length&&!b.sourceIds.some(id=>typeof id!=='string'||!id||id.length>128)&&!Object.keys(b).some(k=>!['documentHash','strokeCount','custom','hadCorrection','sourceIds'].includes(k)))
}
function valid(value:unknown,scope:PendingScope):value is PendingVideoSmartRecord{
 if(!value||typeof value!=='object'||Array.isArray(value))return false
 const v=value as PendingVideoSmartRecord
 if([v.projectId,v.compositionId,v.clipId].some(id=>typeof id!=='string'||!id||id.length>128)||![1,2,3].includes(v.version)||typeof v.id!=='string'||!v.id||v.id.length>128||v.projectId!==scope.projectId||v.compositionId!==scope.compositionId||v.clipId!==scope.clipId||typeof v.sourceHash!=='string'||!/^[a-f0-9]{64}$/.test(v.sourceHash)||!Number.isSafeInteger(v.timeUs)||v.timeUs<0||!['smartBrush','smartEraser'].includes(v.tool)||!Number.isFinite(v.size)||v.size<.005||v.size>.5||!Array.isArray(v.points)||!v.points.length||v.points.length>65536||v.points.some(p=>!Array.isArray(p)||p.length!==2||p.some(x=>!Number.isFinite(x)||x<-.5||x>1.5)))return false
 if(v.version===3)return uuid(v.id)&&validBase(v.base)&&typeof v.sourceId==='string'&&Boolean(v.sourceId)&&v.sourceId.length<=128&&Number.isSafeInteger(v.expectedRevision)&&v.expectedRevision>=0&&!Object.keys(v).some(k=>!['version','base','id','projectId','compositionId','clipId','sourceId','sourceHash','expectedRevision','timeUs','points','tool','size'].includes(k))
 if(v.version===2?!validBase(v.base):v.base!==undefined)return false
 if(v.operationId!==undefined&&(!uuid(v.operationId)||v.operationId!==v.id))return false
 const j=v.started
 if(!j||typeof j.jobId!=='string'||!j.jobId||j.jobId.length>128||typeof j.sourceId!=='string'||!j.sourceId||j.sourceId.length>128||!Number.isSafeInteger(j.sourceUs)||j.sourceUs<0||!Number.isSafeInteger(j.expectedRevision)||j.expectedRevision<0||!Number.isInteger(j.frames)||j.frames<1||j.frames>360000||!Number.isInteger(j.seedFrame)||j.seedFrame<0||j.seedFrame>=j.frames||!j.frameRate||![j.frameRate.numerator,j.frameRate.denominator].every(n=>Number.isInteger(n)&&n>0&&n<=4294967295))return false
 try{if(videoRegionOrdinal(j,j.sourceUs)!==j.seedFrame)return false}catch{return false}
 return !Object.keys(j.frameRate).some(k=>!['numerator','denominator'].includes(k))&&!Object.keys(v).some(k=>!['version','base','operationId','id','projectId','compositionId','clipId','sourceHash','timeUs','points','tool','size','started'].includes(k))&&!Object.keys(j).some(k=>!['jobId','sourceId','sourceUs','seedFrame','frames','frameRate','frameTimesUs','expectedRevision'].includes(k))
}
export function readPendingVideoSmartRecord(store:Store,key:string,scope:PendingScope):PendingVideoSmartRecord|null{
 try{const raw=store.getItem(key);if(!raw||raw.length>MAX_BYTES)return null;const value:unknown=JSON.parse(raw);return valid(value,scope)?value:null}catch{return null}
}
export function readPendingVideoSmart(store:Store,key:string,scope:PendingScope):PendingVideoSmartStroke|null{
 const value=readPendingVideoSmartRecord(store,key,scope);return value&&value.version!==3?value:null
}
export function writePendingVideoSmart(store:Store,key:string,value:PendingVideoSmartRecord):void{
 const encoded=JSON.stringify(value);if(encoded.length>MAX_BYTES||!valid(value,value))throw new Error('Неверная ожидающая правка SAM')
 const raw=store.getItem(key),previous=readPendingVideoSmartRecord(store,key,value);if(raw!==null&&(!previous||previous.id!==value.id))throw new Error('Уже сохранена другая или неизвестная ожидающая правка SAM')
 store.setItem(key,encoded)
}
/** Never clear a newer task written by another panel/tab while this task finishes. */
export function removePendingVideoSmart(store:Store,key:string,id:string):void{
 const raw=store.getItem(key);if(!raw||raw.length>MAX_BYTES)return
 try{if(JSON.parse(raw)?.id===id)store.removeItem(key)}catch{/* Preserve malformed records for manual recovery. */}
}
