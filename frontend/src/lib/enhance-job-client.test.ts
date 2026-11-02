import {afterEach,expect,it,vi} from 'vitest'
import {getImageEnhancementJob} from '$lib/api'
import {observeEnhancement} from './enhance-job-client'
vi.mock('$lib/api',()=>({getImageEnhancementJob:vi.fn()}))
afterEach(()=>{vi.useRealTimers();vi.resetAllMocks()})
it('observes the same owned job through progress to completion',async()=>{
 vi.useFakeTimers();vi.mocked(getImageEnhancementJob).mockResolvedValueOnce({id:'job',status:'running',progress:42}).mockResolvedValueOnce({id:'job',status:'done',progress:100});const update=vi.fn(),controller=new AbortController();const observing=observeEnhancement('owner','job',controller.signal,update);await vi.advanceTimersByTimeAsync(800);expect(await observing).toMatchObject({status:'done'});expect(update.mock.calls.map(call=>call[0].progress)).toEqual([42,100]);expect(getImageEnhancementJob).toHaveBeenCalledTimes(2);expect(getImageEnhancementJob).toHaveBeenLastCalledWith('owner','job',controller.signal);expect(vi.getTimerCount()).toBe(0)
})
it('aborting observation clears its timer and prevents further polling',async()=>{
 vi.useFakeTimers();vi.mocked(getImageEnhancementJob).mockResolvedValue({id:'job',status:'pending'});const controller=new AbortController();const observing=observeEnhancement('owner','job',controller.signal,()=>{});const rejected=expect(observing).rejects.toThrow('detached');await vi.advanceTimersByTimeAsync(0);controller.abort(new Error('detached'));await rejected;await vi.advanceTimersByTimeAsync(2000);expect(getImageEnhancementJob).toHaveBeenCalledTimes(1);expect(vi.getTimerCount()).toBe(0)
})
