import { afterEach, expect, it, vi } from 'vitest'
import { getFilmcraftAnalysis } from '$lib/api'
import { observeAnalysis } from './analysis-client'
vi.mock('$lib/api',()=>({getFilmcraftAnalysis:vi.fn()}))
afterEach(()=>{vi.resetAllMocks();vi.useRealTimers()})
it('polls progress to terminal state without rounding ids',async()=>{
 vi.useFakeTimers();vi.mocked(getFilmcraftAnalysis).mockResolvedValueOnce({id:'18446744073709551615',status:'running',progress:20}).mockResolvedValue({id:'18446744073709551615',status:'done',result:{ready:true}});const update=vi.fn();const controller=new AbortController();const result=observeAnalysis('owner','18446744073709551615',controller.signal,update);await vi.advanceTimersByTimeAsync(800);expect((await result).status).toBe('done');expect(getFilmcraftAnalysis).toHaveBeenCalledWith('owner','18446744073709551615',controller.signal);expect(update).toHaveBeenCalledTimes(2)
})
it('aborts waiting without a further request or update',async()=>{
 vi.useFakeTimers();vi.mocked(getFilmcraftAnalysis).mockResolvedValue({id:'job',status:'running'});const controller=new AbortController();const update=vi.fn();const result=observeAnalysis('owner','job',controller.signal,update);const rejected=expect(result).rejects.toBeDefined();await vi.advanceTimersByTimeAsync(0);controller.abort();await rejected;await vi.advanceTimersByTimeAsync(800);expect(getFilmcraftAnalysis).toHaveBeenCalledTimes(1)
})
it('suppresses a late response after detach',async()=>{let finish!:(value:{id:string;status:'done'})=>void;vi.mocked(getFilmcraftAnalysis).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));const controller=new AbortController();const update=vi.fn();const result=observeAnalysis('owner','job',controller.signal,update);const rejected=expect(result).rejects.toBeDefined();controller.abort();finish({id:'job',status:'done'});await rejected;expect(update).not.toHaveBeenCalled()})
