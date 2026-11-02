import type {AgentActivity,ActivitySession} from './activity.svelte'
export interface ActivitySource {
 workspaceId:string;activities:AgentActivity[];sessions:ActivitySession[];completedIds:string[];evicted:number
 sessionId:string|null;lockReason:string|null;checkpoints:{id:string;label:string}[]
}
export interface ActivitySnapshot extends ActivitySource {revision:number}
export interface ActivityUpdate extends Omit<ActivitySnapshot,'activities'> {activities:AgentActivity[];fromRevision:number;reset:boolean;removedIds:string[];order:string[]}
const copy=<T>(value:T):T=>JSON.parse(JSON.stringify(value)) as T
/** Monotonic across project openings; publishes detached records, never live state/getters. */
export class ActivityPublisher {
 private current:ActivitySnapshot|null=null
 private fingerprint=''
 private listeners=new Set<(update:ActivityUpdate)=>void>()
 constructor(private read:()=>ActivitySource){}
 subscribe(listener:(update:ActivityUpdate)=>void){this.listeners.add(listener);return ()=>{this.listeners.delete(listener)}}
 refresh():ActivitySnapshot {
  const source=copy(this.read()),fingerprint=JSON.stringify(source)
  if(this.current&&fingerprint===this.fingerprint)return this.current
  const previous=this.current,reset=!previous||previous.workspaceId!==source.workspaceId
  const revision=(previous?.revision??0)+1
  const before=new Map((reset?[]:previous!.activities).map(item=>[item.id,JSON.stringify(item)]))
  const retained=new Set(source.activities.map(item=>item.id))
  const update:ActivityUpdate={...source,revision,fromRevision:previous?.revision??0,reset,removedIds:reset?[]:previous!.activities.filter(item=>!retained.has(item.id)).map(item=>item.id),order:source.activities.map(item=>item.id),activities:source.activities.filter(item=>before.get(item.id)!==JSON.stringify(item))}
  this.current={...source,revision};this.fingerprint=fingerprint
  for(const listener of this.listeners)listener(copy(update))
  return this.current
 }
 snapshot():ActivitySnapshot{return copy(this.refresh())}
}
/** Reject stale snapshots/events, including an old workspace arriving after a reset. */
export class ActivityReplica {
 current:ActivitySnapshot|null=null
 snapshot(value:ActivitySnapshot):boolean {
  if(this.current&&value.revision<=this.current.revision)return false
  this.current=copy(value);return true
 }
 update(value:ActivityUpdate):'accepted'|'stale'|'repair' {
  if(this.current&&value.revision<=this.current.revision)return 'stale'
  if(!value.reset&&(!this.current||this.current.workspaceId!==value.workspaceId||this.current.revision!==value.fromRevision))return 'repair'
  const rows=new Map((value.reset?[]:this.current!.activities).map(item=>[item.id,item]))
  for(const id of value.removedIds)rows.delete(id)
  for(const item of value.activities)rows.set(item.id,item)
  if(value.order.length!==rows.size||new Set(value.order).size!==value.order.length||value.order.some(id=>!rows.has(id)))return 'repair'
  const {fromRevision:_,reset:__,removedIds:___,order,...snapshot}=value
  void _;void __;void ___
  this.current=copy({...snapshot,activities:order.map(id=>rows.get(id)!)})
  return 'accepted'
 }
}
export interface ActivityTransport {subscribe(listener:(value:ActivityUpdate)=>void):()=>void;snapshot():ActivitySnapshot|Promise<ActivitySnapshot>}
/** Subscribe first. Serialize repairs and periodically reconcile events lost without a later gap. */
export function wireActivity(transport:ActivityTransport,receive:(snapshot:ActivitySnapshot)=>void,intervalMs=3000):()=>void {
 const replica=new ActivityReplica()
 let disposed=false,pending=false,again=false
 const refresh=async()=>{
  if(pending){again=true;return}
  pending=true
  try{const value=await transport.snapshot();if(!disposed&&replica.snapshot(value))receive(replica.current!)}
  catch{/* Keep the last valid snapshot; the next reconciliation retries. */}
  finally{pending=false;if(again&&!disposed){again=false;void refresh()}}
 }
 const unsubscribe=transport.subscribe(value=>{
  if(disposed)return
  const result=replica.update(value)
  if(result==='repair')void refresh()
  else if(result==='accepted')receive(replica.current!)
 })
 void refresh()
 const timer=setInterval(()=>void refresh(),intervalMs)
 return ()=>{disposed=true;clearInterval(timer);unsubscribe()}
}
