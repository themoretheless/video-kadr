/** Origin-wide ownership for one clip's smart operation, held until observation ends.
 * Older browsers retain single-tab behavior; callers expose the missing guarantee.
 * A closed tab releases its lock; the durable intent remains the recovery authority.
 */
export async function withVideoSmartLock(
 key:string,signal:AbortSignal,work:(atomic:boolean)=>Promise<void>,
 locks:Pick<LockManager,'request'>|undefined=globalThis.navigator?.locks,
):Promise<'completed'|'occupied'|'aborted'>{
 if(signal.aborted)return 'aborted'
 if(!locks){await work(false);return 'completed'}
 return locks.request(`video-kadr.smart-operation:${key}`,{mode:'exclusive',ifAvailable:true},async lock=>{
  if(!lock)return 'occupied'
  if(signal.aborted)return 'aborted'
  await work(true)
  return 'completed'
 })
}
