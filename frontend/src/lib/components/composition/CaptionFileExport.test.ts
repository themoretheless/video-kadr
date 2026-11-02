// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import Export from './CaptionFileExport.svelte'
import {writeCaptionFile} from '$lib/api'
import {createComposition} from '$lib/composition/commands'
vi.mock('$lib/api',()=>({writeCaptionFile:vi.fn()}))
afterEach(()=>{vi.restoreAllMocks();vi.clearAllMocks()})
function composition(){return {...createComposition({width:320,height:180,fps:30000/1001,fpsRate:{numerator:30000,denominator:1001},backgroundColor:'#000000'}),captionTracks:[{id:'first',name:'Other',clips:[]},{id:'active',name:'French/captions',locked:true,vttBlocks:['NOTE retained'],clips:[{id:'internal',cueId:'source-id',timelineStartTick:1,timelineEndTick:1000001,text:'<i>Bonjour</i>',speaker:'Voice',settings:'line:90%'}]}]}}
it('exports selected locked track through native codec with exact rate and metadata; revokes download on unmount',async()=>{
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:caption-export'),revoke=vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})
 vi.mocked(writeCaptionFile).mockResolvedValue({extension:'vtt',dataBase64:btoa('WEBVTT\n')})
 const target=document.createElement('div'),component=mount(Export,{target,props:{composition:composition(),activeId:'active'}});await tick()
 const select=target.querySelector('select')!;select.value='vtt';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 target.querySelector('button')!.click();await vi.waitFor(()=>expect(target.querySelector('a')).not.toBeNull())
 expect(writeCaptionFile).toHaveBeenCalledWith({ticksPerSecond:'254016000000',blocks:['NOTE retained'],cues:[{startTick:'254016',endTick:'254016254016',text:'<i>Bonjour</i>',speaker:'Voice',id:'source-id',settings:'line:90%'}]},'vtt',{dropFrame:true,rate:{num:30000,den:1001}})
 expect(target.querySelector('a')!.download).toBe('French_captions.vtt')
 expect(create).toHaveBeenCalledOnce();expect(await (create.mock.calls[0][0] as Blob).text()).toBe('WEBVTT\n')
 await unmount(component);expect(revoke).toHaveBeenCalledWith('blob:caption-export')
})
it('surfaces native write failure and does not expose a download',async()=>{
 vi.mocked(writeCaptionFile).mockRejectedValue(new Error('codec failed'))
 const target=document.createElement('div'),component=mount(Export,{target,props:{composition:composition()}});await tick()
 target.querySelector('button')!.click();await vi.waitFor(()=>expect(target.querySelector('[role=alert]')?.textContent).toContain('codec failed'))
 expect(target.querySelector('a')).toBeNull();expect(target.querySelector('button')!.disabled).toBe(false)
 await unmount(component)
})
it('ignores a completed export after panel unmount and creates no orphan download',async()=>{
 let resolve!:(value:{extension:'srt';dataBase64:string})=>void
 vi.mocked(writeCaptionFile).mockReturnValue(new Promise(done=>{resolve=done}))
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:orphan')
 const target=document.createElement('div'),component=mount(Export,{target,props:{composition:composition()}});await tick()
 target.querySelector('button')!.click();await tick();expect(writeCaptionFile).toHaveBeenCalledOnce()
 expect(target.querySelector('button')!.disabled).toBe(true)
 await unmount(component);resolve({extension:'srt',dataBase64:btoa('export')});await tick();await Promise.resolve()
 expect(create).not.toHaveBeenCalled()
})
it('downloads binary STL bytes unchanged and passes non-drop-frame selection',async()=>{
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:binary-caption');vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})
 vi.mocked(writeCaptionFile).mockResolvedValue({extension:'stl',dataBase64:btoa(String.fromCharCode(0,128,255,13,10))})
 const target=document.createElement('div'),component=mount(Export,{target,props:{composition:composition(),activeId:'active'}});await tick()
 const select=target.querySelector('select')!;select.value='stl';select.dispatchEvent(new Event('change',{bubbles:true}))
 const checkbox=target.querySelector<HTMLInputElement>('input[type=checkbox]')!;checkbox.checked=false;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 target.querySelector('button')!.click();await vi.waitFor(()=>expect(create).toHaveBeenCalledOnce())
 expect(vi.mocked(writeCaptionFile).mock.calls.at(-1)![1]).toBe('stl')
 expect(vi.mocked(writeCaptionFile).mock.calls.at(-1)![2]?.dropFrame).toBe(false)
 expect([...new Uint8Array(await (create.mock.calls[0][0] as Blob).arrayBuffer())]).toEqual([0,128,255,13,10])
 expect(target.querySelector('a')!.download).toBe('French_captions.stl')
 await unmount(component)
})
