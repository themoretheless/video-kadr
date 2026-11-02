import {expect,it} from 'vitest'
import {pendingVideoSmartKey,readPendingVideoSmart,readPendingVideoSmartRecord,writePendingVideoSmart,removePendingVideoSmart,type PendingVideoSmartStroke,type PendingVideoSmartSubmission} from './pendingVideoSmart'
const scope={projectId:'project',compositionId:'root',clipId:'clip'}
function record():PendingVideoSmartStroke{return {version:1,id:'intent',...scope,sourceHash:'a'.repeat(64),timeUs:0,points:[[.5,.5]],tool:'smartBrush',size:.1,started:{jobId:'job',sourceId:'raw',sourceUs:100000,seedFrame:1,frames:3,frameRate:{numerator:10,denominator:1},expectedRevision:7}}}
function store(){const data=new Map<string,string>();return {getItem:(key:string)=>data.get(key)??null,setItem:(key:string,value:string)=>{data.set(key,value)},removeItem:(key:string)=>{data.delete(key)}}}
it('isolates sessions and clip scopes without persisting a credential',async()=>{
 const key=await pendingVideoSmartKey('private-session-a',scope);expect(key).not.toContain('private-session-a');expect(await pendingVideoSmartKey('private-session-a',scope)).toBe(key);expect(await pendingVideoSmartKey('private-session-b',scope)).not.toBe(key);expect(await pendingVideoSmartKey('private-session-a',{...scope,clipId:'other'})).not.toBe(key);const storage=store();writePendingVideoSmart(storage,key,record());expect(storage.getItem(key)).not.toContain('private-session-a');expect(readPendingVideoSmart(storage,key,scope)).toEqual(record());expect(readPendingVideoSmart(storage,key,{...scope,projectId:'other'})).toBeNull()
})
it('does not remove a newer accepted task when an older observer finishes',()=>{
 const storage=store(),first=record(),second={...record(),id:'new-intent'};writePendingVideoSmart(storage,'key',first);storage.setItem('key',JSON.stringify(second));removePendingVideoSmart(storage,'key',first.id);expect(readPendingVideoSmart(storage,'key',scope)).toEqual(second);removePendingVideoSmart(storage,'key',second.id);expect(readPendingVideoSmart(storage,'key',scope)).toBeNull()
})
it('rejects malformed and inconsistent records without destroying their bytes',()=>{
 const storage=store();for(const value of [{...record(),size:2},{...record(),sourceHash:'wrong'},{...record(),tool:'brush'},{...record(),started:{...record().started,seedFrame:0}},{...record(),points:[[Infinity,0]]},{...record(),extra:'untrusted'}]){const raw=JSON.stringify(value);storage.setItem('key',raw);expect(readPendingVideoSmart(storage,'key',scope)).toBeNull();expect(storage.getItem('key')).toBe(raw)}
 storage.setItem('key','unfinished JSON');expect(readPendingVideoSmart(storage,'key',scope)).toBeNull();removePendingVideoSmart(storage,'key','intent');expect(storage.getItem('key')).toBe('unfinished JSON')
})

it('preserves an existing accepted intent if a second writer arrives',()=>{
 const storage=store(),first=record();writePendingVideoSmart(storage,'key',first);expect(()=>writePendingVideoSmart(storage,'key',{...record(),id:'other'})).toThrow('другая');expect(readPendingVideoSmart(storage,'key',scope)).toEqual(first)
})

it('does not overwrite an unknown or damaged record with a newly accepted task',()=>{
 const storage=store();storage.setItem('key','recoverable unknown data');expect(()=>writePendingVideoSmart(storage,'key',record())).toThrow('неизвестная');expect(storage.getItem('key')).toBe('recoverable unknown data')
})

it('roundtrips a v2 document receipt and preserves malformed receipts',()=>{
 const storage=store(),value={...record(),version:2 as const,base:{documentHash:'b'.repeat(64),strokeCount:0,custom:false,hadCorrection:false,sourceIds:['raw']}}
 writePendingVideoSmart(storage,'key',value);expect(readPendingVideoSmart(storage,'key',scope)).toEqual(value)
 for(const base of [{...value.base,documentHash:'bad'},{...value.base,strokeCount:1024},{...value.base,sourceIds:['raw','raw']},{...value.base,extra:true}]){const raw=JSON.stringify({...value,base});storage.setItem('key',raw);expect(readPendingVideoSmart(storage,'key',scope)).toBeNull();expect(storage.getItem('key')).toBe(raw)}
})

it('retains accepted VFR seed clocks instead of recomputing an average-FPS ordinal',()=>{
 const storage=store(),value=record();value.started={...value.started,sourceUs:250000,seedFrame:1,frameTimesUs:[0,100000,400000,500000]}
 writePendingVideoSmart(storage,'key',value);expect(readPendingVideoSmart(storage,'key',scope)).toEqual(value)
 storage.setItem('key',JSON.stringify({...value,started:{...value.started,frameTimesUs:[0,100000,200000,500000]}}));expect(readPendingVideoSmart(storage,'key',scope)).toBeNull()
})

it('atomically promotes the same pre-acknowledgement record without changing its identity',()=>{
 const storage=store(),id=crypto.randomUUID(),base={documentHash:'b'.repeat(64),strokeCount:0,custom:false,hadCorrection:false,sourceIds:['raw']}
 const draft:PendingVideoSmartSubmission={version:3,id,...scope,base,sourceId:'raw',sourceHash:'a'.repeat(64),expectedRevision:7,timeUs:0,points:[[.5,.5]],tool:'smartBrush',size:.1}
 writePendingVideoSmart(storage,'key',draft);expect(readPendingVideoSmartRecord(storage,'key',scope)).toEqual(draft);expect(readPendingVideoSmart(storage,'key',scope)).toBeNull()
 expect(()=>writePendingVideoSmart(storage,'key',{...draft,id:crypto.randomUUID()})).toThrow('другая')
 const accepted={...record(),version:2 as const,id,operationId:id,base};writePendingVideoSmart(storage,'key',accepted);expect(readPendingVideoSmart(storage,'key',scope)).toEqual(accepted)
 for(const value of [{...draft,expectedRevision:-1},{...draft,id:'invalid'},{...draft,started:record().started},{...draft,sourceId:''}]){const raw=JSON.stringify(value);storage.setItem('key',raw);expect(readPendingVideoSmartRecord(storage,'key',scope)).toBeNull();expect(storage.getItem('key')).toBe(raw)}
})
