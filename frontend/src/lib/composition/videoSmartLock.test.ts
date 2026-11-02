import {expect,it,vi} from 'vitest'
import {withVideoSmartLock} from './videoSmartLock'
function manager(){
 const held=new Set<string>()
 return {request:vi.fn(async(name:string,_options:unknown,work:(lock:unknown)=>Promise<unknown>)=>{
  if(held.has(name))return work(null)
  held.add(name);try{return await work({name,mode:'exclusive'})}finally{held.delete(name)}
 })} as unknown as Pick<LockManager,'request'>
}
it('holds clip ownership until completion, isolates keys and releases on errors',async()=>{
 const locks=manager(),signal=new AbortController().signal;let finish!:()=>void
 const work=vi.fn(()=>new Promise<void>(resolve=>{finish=resolve})),blocked=vi.fn()
 const first=withVideoSmartLock('clip',signal,work,locks)
 expect(work).toHaveBeenCalledWith(true)
 expect(await withVideoSmartLock('clip',signal,blocked,locks)).toBe('occupied');expect(blocked).not.toHaveBeenCalled()
 expect(await withVideoSmartLock('other',signal,blocked,locks)).toBe('completed')
 finish();expect(await first).toBe('completed')
 await expect(withVideoSmartLock('clip',signal,async()=>{throw new Error('failed')},locks)).rejects.toThrow('failed')
 expect(await withVideoSmartLock('clip',signal,async()=>{},locks)).toBe('completed')
})
it('does not start an aborted operation and exposes legacy fallback',async()=>{
 const controller=new AbortController(),work=vi.fn();controller.abort()
 expect(await withVideoSmartLock('clip',controller.signal,work,manager())).toBe('aborted');expect(work).not.toHaveBeenCalled()
 expect(await withVideoSmartLock('clip',new AbortController().signal,work,undefined)).toBe('completed');expect(work).toHaveBeenCalledWith(false)
})
