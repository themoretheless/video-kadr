import {afterEach,expect,it,vi} from 'vitest'
import {ActivityPublisher,ActivityReplica,wireActivity,type ActivitySource,type ActivitySnapshot,type ActivityUpdate} from './activitySync'
import type {AgentActivity} from './activity.svelte'
const item=(id:string,state:AgentActivity['state']='done'):AgentActivity=>({id,actor:'a',tool:'get_project',kind:'read',sessionId:null,startedAt:1,endedAt:state==='running'?null:2,durationMs:state==='running'?null:1,state,args:'{}',error:null,historyIds:[],effect:null})
const source=(workspaceId='first'):ActivitySource=>({workspaceId,activities:[],sessions:[],completedIds:[],evicted:0,sessionId:null,lockReason:null,checkpoints:[]})
afterEach(()=>vi.useRealTimers())
it('publishes detached deltas, removals, order and monotonic resets across openings',()=>{
 let state=source();const publisher=new ActivityPublisher(()=>state),updates:ActivityUpdate[]=[]
 publisher.subscribe(update=>updates.push(update))
 const first=publisher.snapshot();expect(first.revision).toBe(1)
 state={...state,activities:[item('old','running'),item('new')],completedIds:['new']}
 const second=publisher.snapshot();const replica=new ActivityReplica();replica.snapshot(first)
 expect(replica.update(updates.at(-1)!)).toBe('accepted');expect(replica.current).toEqual(second)
 state={...state,activities:[item('old')],completedIds:['old'],evicted:1}
 const third=publisher.snapshot();expect(updates.at(-1)!.removedIds).toEqual(['new']);expect(updates.at(-1)!.order).toEqual(['old'])
 expect(replica.update(updates.at(-1)!)).toBe('accepted');expect(replica.current).toEqual(third)
 third.activities[0]!.actor='mutated';expect(publisher.snapshot().activities[0]!.actor).toBe('a')
 expect(publisher.snapshot().revision).toBe(third.revision)
 state=source('second');const reset=publisher.snapshot()
 expect(reset.revision).toBeGreaterThan(second.revision);expect(updates.at(-1)!.reset).toBe(true)
 expect(replica.update(updates.at(-1)!)).toBe('accepted')
 expect(replica.snapshot(second)).toBe(false);expect(replica.current?.workspaceId).toBe('second')
})
it('refuses revision gaps, mismatched workspaces, and incomplete ordering without mutating the replica',()=>{
 let state=source();const publisher=new ActivityPublisher(()=>state),updates:ActivityUpdate[]=[];publisher.subscribe(value=>updates.push(value))
 const first=publisher.snapshot(),replica=new ActivityReplica();replica.snapshot(first)
 state={...state,activities:[item('a')],completedIds:['a']};publisher.snapshot()
 state={...state,activities:[item('a'),item('b')],completedIds:['a','b']};publisher.snapshot()
 expect(replica.update(updates.at(-1)!)).toBe('repair');expect(replica.current).toEqual(first)
 expect(replica.update({...updates[1]!,workspaceId:'foreign'})).toBe('repair')
 expect(replica.update({...updates[1]!,order:['missing']})).toBe('repair')
 expect(replica.current).toEqual(first)
 expect(replica.update(updates[1]!)).toBe('accepted')
 expect(replica.update(updates[1]!)).toBe('stale')
})
it('subscribes before loading; a late old snapshot cannot replace a newer reset event',async()=>{
 vi.useFakeTimers()
 let state=source(),listener!:(update:ActivityUpdate)=>void,resolve!:(value:ActivitySnapshot)=>void
 const publisher=new ActivityPublisher(()=>state),old=publisher.snapshot(),receive=vi.fn()
 publisher.subscribe(value=>listener(value))
 const subscribe=vi.fn((callback:(update:ActivityUpdate)=>void)=>{listener=callback;return vi.fn()})
 const stop=wireActivity({subscribe,snapshot:()=>new Promise(done=>{resolve=done})},receive)
 expect(subscribe).toHaveBeenCalledTimes(1)
 state=source('new-opening');const latest=publisher.snapshot()
 expect(receive).toHaveBeenLastCalledWith(latest)
 resolve(old);await Promise.resolve();await Promise.resolve()
 expect(receive).toHaveBeenCalledTimes(1)
 stop();expect(vi.getTimerCount()).toBe(0)
})
it('repairs dropped events periodically, retries snapshot errors, and disposes pending replies',async()=>{
 vi.useFakeTimers()
 let state=source(),drop=false,listener!:(value:ActivityUpdate)=>void
 const publisher=new ActivityPublisher(()=>state)
 publisher.subscribe(update=>{if(!drop)listener(update)})
 const unsubscribe=vi.fn(),receive=vi.fn()
 const snapshot=vi.fn(()=>publisher.snapshot())
 const stop=wireActivity({subscribe(callback){listener=callback;return unsubscribe},snapshot},receive,100)
 await Promise.resolve();await Promise.resolve()
 const count=receive.mock.calls.length
 drop=true;state={...state,activities:[item('silent')],completedIds:['silent']};publisher.snapshot()
 snapshot.mockImplementationOnce(()=>{throw new Error('temporarily unavailable')})
 await vi.advanceTimersByTimeAsync(100);expect(receive).toHaveBeenCalledTimes(count)
 await vi.advanceTimersByTimeAsync(100);expect(receive.mock.lastCall?.[0].activities[0].id).toBe('silent')
 stop();expect(unsubscribe).toHaveBeenCalledTimes(1);expect(vi.getTimerCount()).toBe(0)
})
it('coalesces concurrent gap repairs and ignores replies after disposal',async()=>{
 vi.useFakeTimers()
 const state=source();let listener!:(value:ActivityUpdate)=>void,resolve!:(value:ActivitySnapshot)=>void
 const publisher=new ActivityPublisher(()=>state),initial=publisher.snapshot(),receive=vi.fn()
 const snapshot=vi.fn(()=>new Promise<ActivitySnapshot>(done=>{resolve=done}))
 const stop=wireActivity({subscribe(callback){listener=callback;return ()=>{}},snapshot},receive,100)
 const gap:ActivityUpdate={...initial,revision:10,fromRevision:9,reset:false,removedIds:[],order:[],activities:[]}
 listener(gap);listener(gap);expect(snapshot).toHaveBeenCalledTimes(1)
 stop();resolve(initial);await Promise.resolve();await Promise.resolve()
 expect(receive).not.toHaveBeenCalled();expect(snapshot).toHaveBeenCalledTimes(1);expect(vi.getTimerCount()).toBe(0)
})

it('repairs a later event gap with one fresh snapshot before accepting subsequent deltas',async()=>{
 vi.useFakeTimers()
 let state=source(),listener!:(value:ActivityUpdate)=>void,drop=false
 const publisher=new ActivityPublisher(()=>state),receive=vi.fn()
 publisher.subscribe(update=>{if(!drop)listener(update)})
 const snapshot=vi.fn(()=>publisher.snapshot())
 const stop=wireActivity({subscribe(callback){listener=callback;return ()=>{}},snapshot},receive)
 await Promise.resolve();await Promise.resolve()
 drop=true;state={...state,activities:[item('missed')],completedIds:['missed']};publisher.snapshot()
 drop=false;state={...state,activities:[item('missed'),item('later')],completedIds:['missed','later']};publisher.snapshot()
 await Promise.resolve();await Promise.resolve()
 expect(snapshot).toHaveBeenCalledTimes(2)
 expect(receive.mock.lastCall?.[0].activities.map((row:AgentActivity)=>row.id)).toEqual(['missed','later'])
 state={...state,lockReason:'Owned lock'};publisher.snapshot()
 expect(receive.mock.lastCall?.[0].lockReason).toBe('Owned lock')
 stop()
})
