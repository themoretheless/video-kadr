// @ts-expect-error Tests need Svelte client mount under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import {exportConcatProject} from '$lib/api'
import {nestedComposition,closeNestedComposition} from '$lib/state/nestedComposition.svelte'
import ConcatProjectExportPanel from './ConcatProjectExportPanel.svelte'
vi.mock('$lib/api',()=>({exportConcatProject:vi.fn()}))
afterEach(()=>{closeNestedComposition();vi.restoreAllMocks();vi.clearAllMocks();document.body.innerHTML=''})
function setup(){closeNestedComposition();nestedComposition.document={schemaVersion:1,root:'root',compositions:{}};nestedComposition.activeCompositionId='active';nestedComposition.name='Source export'}
it('downloads the Rust source document with the active timeline without changing the native project',async()=>{
 setup();const original=nestedComposition.document,source={version:1,timelines:[]};vi.mocked(exportConcatProject).mockResolvedValue({schemaVersion:1,document:source})
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:concat'),revoke=vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{}),click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(()=>{})
 const component=mount(ConcatProjectExportPanel,{target:document.body})
 try{await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(click).toHaveBeenCalledOnce());expect(exportConcatProject).toHaveBeenCalledWith({...original,root:'active'},'Source export',expect.any(AbortSignal),{});expect(await (create.mock.calls[0][0] as Blob).text()).toBe(JSON.stringify(source));expect((click.mock.contexts[0] as HTMLAnchorElement).download).toBe('concat.json');expect(revoke).toHaveBeenCalledWith('blob:concat');expect(nestedComposition.document).toBe(original)}finally{await unmount(component)}
})
it('shows native incompatibility diagnostics and permits retry',async()=>{
 setup();vi.mocked(exportConcatProject).mockRejectedValue(Error('clip look: native animation cannot be exported losslessly'));const component=mount(ConcatProjectExportPanel,{target:document.body})
 try{await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('cannot be exported losslessly'));expect(document.querySelector('button')!.disabled).toBe(false)}finally{await unmount(component)}
})
it('aborts an export on disposal and refuses a late download',async()=>{
 setup();let resolve!:(value:{schemaVersion:1;document:unknown})=>void;vi.mocked(exportConcatProject).mockReturnValue(new Promise(done=>resolve=done));const create=vi.spyOn(URL,'createObjectURL');const component=mount(ConcatProjectExportPanel,{target:document.body});await tick();document.querySelector('button')!.click();await tick();const signal=vi.mocked(exportConcatProject).mock.calls[0][2]!
 await unmount(component);expect(signal.aborted).toBe(true);resolve({schemaVersion:1,document:{}});await tick();expect(create).not.toHaveBeenCalled()
})
it('requires explicit media paths and snapshots the mapping into the export request',async()=>{
 setup();nestedComposition.document!.compositions.root={instances:[],content:{schemaVersion:1,timeBase:1000000,canvas:{width:16,height:16,fpsMilli:25000,background:{red:0,green:0,blue:0,alpha:1},backgroundMode:'color',backgroundBlur:0},sources:{sound:{id:'sound',kind:'audio',durationTicks:1000000,width:0,height:0,hasAudio:true}},tracks:[]}}
 vi.mocked(exportConcatProject).mockRejectedValue(Error('missing path'));const component=mount(ConcatProjectExportPanel,{target:document.body})
 try{await tick();const input=document.querySelector('input')!;expect(input.getAttribute('aria-label')).toBe('Путь медиа Concat: sound');input.value='/portable/source.wav';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(exportConcatProject).toHaveBeenCalled());expect(vi.mocked(exportConcatProject).mock.calls[0][3]).toEqual({sound:'/portable/source.wav'})}finally{await unmount(component)}
})
