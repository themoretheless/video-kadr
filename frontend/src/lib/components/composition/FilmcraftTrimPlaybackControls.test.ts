// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { ApiError, createFilmcraftTrimSession,commandFilmcraftTrimSession,readFilmcraftTrimSession,closeFilmcraftTrimSession,type FilmcraftTrimSessionResult } from '$lib/api'
import Controls from './FilmcraftTrimPlaybackControls.svelte'
vi.mock('$lib/api',async importOriginal=>({...await importOriginal<typeof import('$lib/api')>(),createFilmcraftTrimSession:vi.fn(),commandFilmcraftTrimSession:vi.fn(),readFilmcraftTrimSession:vi.fn(),closeFilmcraftTrimSession:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const base={documentText:'{"base":true}',originalSchema:12,sequences:[],media:[]}
const result=(revision:string,dynamic:boolean,doc='{"base":true}',commit=false):FilmcraftTrimSessionResult=>({...base,documentText:doc,sessionId:'session',revision,shift:{outShift:'0',inShift:'0'},shiftFrames:{outShift:'0',inShift:'0'},status:{playing:dynamic,playheadTicks:'9007199254740993',playheadFrames:'851020',dynamic:dynamic?{offsetFrames:'2',speed:1,atLimit:false}:null,around:null},commit:commit?{committed:true,label:'Dynamic Trim',offsetTicks:'2',offsetFrames:'2'}:null})
afterEach(async()=>{if(component)await unmount(component);component=undefined;vi.restoreAllMocks();vi.resetAllMocks();document.body.innerHTML=''})
function setup(){vi.spyOn(globalThis,'setInterval').mockImplementation(()=>0 as unknown as ReturnType<typeof setInterval>);vi.mocked(createFilmcraftTrimSession).mockResolvedValue(result('0',false));vi.mocked(readFilmcraftTrimSession).mockResolvedValue(result('9',false));vi.mocked(closeFilmcraftTrimSession).mockResolvedValue({closed:true});const callbacks={onPreview:vi.fn(),onCommit:vi.fn(),onActive:vi.fn(),onError:vi.fn()};component=mount(Controls,{target:document.body,props:{token:'owner',project:base,input:{document:base.documentText,sequenceId:'1',points:[{clip:'2',out:true,kind:'trim'}],linked:true,shift:{outShift:'0',inShift:'0'},playheadTicks:'0'},...callbacks}});return callbacks}
const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!
it('previews multiple updates but emits one baseline commit on stop',async()=>{
 const c=setup();vi.mocked(commandFilmcraftTrimSession).mockResolvedValueOnce(result('1',true,'{"live":1}')).mockResolvedValueOnce(result('2',true,'{"live":2}')).mockResolvedValueOnce(result('3',false,'{"live":2}',true))
 button('Вперёд (L)').click();await vi.waitFor(()=>expect(c.onPreview).toHaveBeenCalledTimes(1));expect(c.onCommit).not.toHaveBeenCalled();button('Вперёд (L)').click();await vi.waitFor(()=>expect(c.onPreview).toHaveBeenCalledTimes(2));expect(c.onCommit).not.toHaveBeenCalled();button('Стоп (K)').click();await vi.waitFor(()=>expect(c.onCommit).toHaveBeenCalledTimes(1));expect(c.onCommit.mock.calls[0]![0]).toEqual(base);expect(c.onCommit.mock.calls[0]![1].documentText).toBe('{"live":2}');expect(c.onActive).toHaveBeenCalledWith(true)
 button('Закрыть монтажную сессию').click();await vi.waitFor(()=>expect(c.onActive).toHaveBeenLastCalledWith(false));expect(closeFilmcraftTrimSession).toHaveBeenCalledWith('owner','session','3',expect.any(AbortSignal))
})
it('cancels without committing and supports focused keyboard controls',async()=>{
 const c=setup();vi.mocked(commandFilmcraftTrimSession).mockResolvedValueOnce(result('1',true,'{"live":1}')).mockResolvedValueOnce(result('2',false))
 document.querySelector('[aria-label="Динамическая подрезка Filmcraft"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'j',bubbles:true}));await vi.waitFor(()=>expect(c.onPreview).toHaveBeenCalledTimes(1));expect(vi.mocked(commandFilmcraftTrimSession).mock.calls[0]![1].action).toMatchObject({op:'shuttle',direction:'reverse'})
 button('Отменить динамическую подрезку').click();await vi.waitFor(()=>expect(c.onPreview).toHaveBeenCalledTimes(2));expect(c.onCommit).not.toHaveBeenCalled();expect(c.onPreview.mock.calls.at(-1)![0].documentText).toBe(base.documentText)
})
it('recovers an uncertain stop and emits the pending commit once',async()=>{
 const c=setup();vi.mocked(commandFilmcraftTrimSession).mockResolvedValueOnce(result('1',true,'{"live":1}')).mockRejectedValueOnce(new Error('connection lost'));button('Вперёд (L)').click();await vi.waitFor(()=>expect(c.onPreview).toHaveBeenCalledTimes(1));button('Стоп (K)').click();await vi.waitFor(()=>expect(c.onError).toHaveBeenCalled());expect(button('Вперёд (L)').disabled).toBe(true)
 vi.mocked(readFilmcraftTrimSession).mockResolvedValue(result('2',false,'{"live":1}'));button('Восстановить состояние сессии').click();await vi.waitFor(()=>expect(c.onCommit).toHaveBeenCalledTimes(1));expect(c.onCommit.mock.calls[0]![0]).toEqual(base);await tick()
})

it('recovers a lost creation response using the same frozen request before sending commands',async()=>{
 const c=setup();vi.mocked(createFilmcraftTrimSession).mockRejectedValueOnce(new Error('creation response lost')).mockResolvedValueOnce(result('0',false));
 button('Вперёд (L)').click();await vi.waitFor(()=>expect(c.onError).toHaveBeenCalled());expect(commandFilmcraftTrimSession).not.toHaveBeenCalled();expect(c.onActive).toHaveBeenLastCalledWith(true);expect(button('Вперёд (L)').disabled).toBe(true);
 const original=vi.mocked(createFilmcraftTrimSession).mock.calls[0]![1];expect(original.creationKey).toMatch(/^[0-9a-f-]{36}$/);
 button('Восстановить состояние сессии').click();await vi.waitFor(()=>expect(c.onPreview).toHaveBeenCalledTimes(1));expect(vi.mocked(createFilmcraftTrimSession).mock.calls[1]![1]).toEqual(original);expect(commandFilmcraftTrimSession).not.toHaveBeenCalled();expect(c.onCommit).not.toHaveBeenCalled();
 button('Закрыть монтажную сессию').click();await vi.waitFor(()=>expect(c.onActive).toHaveBeenLastCalledWith(false));
})

it('releases the edit lock after a definitive creation refusal',async()=>{
 const c=setup();vi.mocked(createFilmcraftTrimSession).mockRejectedValueOnce(new ApiError('invalid creation',400));
 button('Вперёд (L)').click();await vi.waitFor(()=>expect(c.onError).toHaveBeenCalled());expect(c.onActive).toHaveBeenLastCalledWith(false);expect(button('Восстановить состояние сессии')).toBeUndefined();expect(button('Вперёд (L)').disabled).toBe(false);expect(commandFilmcraftTrimSession).not.toHaveBeenCalled();
})
