import { beforeEach, expect, it, vi } from 'vitest'
import { ApiError, createFilmcraftTrimSession,readFilmcraftTrimSession,commandFilmcraftTrimSession,closeFilmcraftTrimSession,type FilmcraftTrimSessionResult } from '$lib/api'
import { FilmcraftTrimSessionClient } from './trim-session-client'
vi.mock('$lib/api',async importOriginal=>({...await importOriginal<typeof import('$lib/api')>(),createFilmcraftTrimSession:vi.fn(),readFilmcraftTrimSession:vi.fn(),commandFilmcraftTrimSession:vi.fn(),closeFilmcraftTrimSession:vi.fn()}))
const state=(revision:string)=>({sessionId:'session',revision,documentText:`{"revision":"${revision}"}`} as FilmcraftTrimSessionResult)
const input={document:'opaque',sequenceId:'1',points:[],linked:true,shift:{outShift:'0',inShift:'0'},playheadTicks:'0'}
beforeEach(()=>{vi.clearAllMocks();vi.mocked(createFilmcraftTrimSession).mockResolvedValue(state('9007199254740993'))})
it('serializes commands using the last acknowledged exact revision',async()=>{
 const client=await FilmcraftTrimSessionClient.create('owner',input)
 let release!:(value:FilmcraftTrimSessionResult)=>void
 vi.mocked(commandFilmcraftTrimSession).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve})).mockResolvedValueOnce(state('9007199254740995'))
 const first=client.command({op:'shuttle',clock:0,direction:'forward',slow:false});const second=client.command({op:'tick',clock:0.1})
 await vi.waitFor(()=>expect(commandFilmcraftTrimSession).toHaveBeenCalledTimes(1));expect(vi.mocked(commandFilmcraftTrimSession).mock.calls[0]![1].revision).toBe('9007199254740993')
 release(state('9007199254740994'));await first;await second
 expect(vi.mocked(commandFilmcraftTrimSession).mock.calls[1]![1].revision).toBe('9007199254740994');expect(client.state.revision).toBe('9007199254740995')
})
it('blocks further writes after uncertain response until authoritative recovery',async()=>{
 const client=await FilmcraftTrimSessionClient.create('owner',input);vi.mocked(commandFilmcraftTrimSession).mockRejectedValueOnce(new Error('connection lost'))
 await expect(client.command({op:'stop'})).rejects.toThrow('connection lost');expect(client.needsRecovery).toBe(true)
 await expect(client.command({op:'cancel'})).rejects.toThrow('восстановить');expect(commandFilmcraftTrimSession).toHaveBeenCalledTimes(1)
 vi.mocked(readFilmcraftTrimSession).mockResolvedValue(state('9007199254740994'));await client.recover();expect(client.needsRecovery).toBe(false)
 vi.mocked(commandFilmcraftTrimSession).mockResolvedValue(state('9007199254740995'));await client.command({op:'cancel'});expect(vi.mocked(commandFilmcraftTrimSession).mock.calls.at(-1)![1].revision).toBe('9007199254740994')
})
it('does not adopt a late response after cancellation',async()=>{
 const client=await FilmcraftTrimSessionClient.create('owner',input);let release!:(value:FilmcraftTrimSessionResult)=>void
 vi.mocked(commandFilmcraftTrimSession).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}));const work=client.command({op:'tick',clock:1});await vi.waitFor(()=>expect(commandFilmcraftTrimSession).toHaveBeenCalledTimes(1));client.cancelRequest();release(state('9007199254740994'));await expect(work).rejects.toThrow('отменён');expect(client.state.revision).toBe('9007199254740993');expect(client.needsRecovery).toBe(true)
})
it('preserves the last acknowledged document when recovery confirms expiry',async()=>{
 const client=await FilmcraftTrimSessionClient.create('owner',input);vi.mocked(readFilmcraftTrimSession).mockRejectedValue(new ApiError('expired',404));expect(await client.recover()).toBeNull();expect(client.isClosed).toBe(true);expect(client.state.documentText).toContain('9007199254740993');await expect(client.command({op:'cancel'})).rejects.toThrow('закрыта')
})
it('closes once with current exact revision and rejects later commands',async()=>{
 const client=await FilmcraftTrimSessionClient.create('owner',input);vi.mocked(closeFilmcraftTrimSession).mockResolvedValue({closed:true});await client.close();await client.close();expect(closeFilmcraftTrimSession).toHaveBeenCalledTimes(1);expect(closeFilmcraftTrimSession).toHaveBeenCalledWith('owner','session','9007199254740993',expect.any(AbortSignal));await expect(client.command({op:'cancel'})).rejects.toThrow('закрыта')
})
