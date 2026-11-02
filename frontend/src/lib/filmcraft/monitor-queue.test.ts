import { expect, it, vi } from 'vitest'
import { MonitorQueue } from './monitor-queue'
const flush = async () => { for (let i=0;i<8;i++) await Promise.resolve() }
it('finishes a slow frame and coalesces rapid updates into the last position', async () => {
  const loads: { input: number; signal: AbortSignal; resolve: (n:number)=>void }[]=[]
  const result=vi.fn();const queue=new MonitorQueue<number,number>((input,signal)=>new Promise(resolve=>loads.push({input,signal,resolve})),{reset:vi.fn(),result,error:vi.fn(),pending:vi.fn()})
  queue.submit('clip',0);await flush();queue.submit('clip',0);for(let i=1;i<=20;i++)queue.submit('clip',i)
  expect(loads).toHaveLength(1);expect(loads[0]!.signal.aborted).toBe(false)
  loads[0]!.resolve(0);await flush();expect(result).toHaveBeenCalledWith(0);expect(loads.map(v=>v.input)).toEqual([0,20])
  loads[1]!.resolve(20);await flush();expect(result).toHaveBeenLastCalledWith(20);queue.submit('clip',20);await flush();expect(loads).toHaveLength(2);queue.clear()
})
it('aborts changed selections and ignores late results after close', async () => {
  const loads: { signal: AbortSignal; resolve: (n:number)=>void }[]=[];const result=vi.fn()
  const queue=new MonitorQueue<number,number>((_,signal)=>new Promise(resolve=>loads.push({signal,resolve})),{reset:vi.fn(),result,error:vi.fn(),pending:vi.fn()})
  queue.submit('a',0);await flush();queue.submit('a',1);queue.submit('b',2);await flush();expect(loads[0]!.signal.aborted).toBe(true)
  loads[0]!.resolve(0);await flush();expect(result).not.toHaveBeenCalled();queue.clear();loads[1]!.resolve(2);await flush();expect(result).not.toHaveBeenCalled();expect(loads).toHaveLength(2)
})
it('continues to the latest position after a superseded decode failure', async () => {
  let fail!:(e:Error)=>void;const error=vi.fn();const result=vi.fn()
  const load=vi.fn().mockImplementationOnce(()=>new Promise((_,reject)=>{fail=reject})).mockResolvedValue(2)
  const queue=new MonitorQueue<number,number>(load,{reset:vi.fn(),result,error,pending:vi.fn()})
  queue.submit('a',1);await flush();queue.submit('a',2);fail(new Error('old decode failed'));await flush();expect(error).not.toHaveBeenCalled();expect(result).toHaveBeenCalledWith(2);queue.clear()
})
