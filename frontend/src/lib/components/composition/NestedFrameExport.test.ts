// @ts-expect-error Tests need Svelte client mount under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import {exportNestedFramePng,type NativeNestedProject} from '$lib/api'
import {nestedComposition,nestedPlayback,closeNestedComposition} from '$lib/state/nestedComposition.svelte'
import NestedFrameExport from './NestedFrameExport.svelte'
vi.mock('$lib/api',()=>({exportNestedFramePng:vi.fn()}))
afterEach(()=>{closeNestedComposition();vi.restoreAllMocks();vi.clearAllMocks();document.body.innerHTML=''})
function setup(){closeNestedComposition();nestedComposition.document={schemaVersion:1,root:'root',compositions:{}} as NativeNestedProject;nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1250000;nestedComposition.projectId='saved';nestedComposition.revision=3}
it('pauses playback, exports the captured native frame and saved revision without editing history',async()=>{
 setup();nestedPlayback.playing=true
 vi.mocked(exportNestedFramePng).mockResolvedValue(new Blob(['png'],{type:'image/png'}))
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:frame'),revoke=vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{}),click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(()=>{})
 const component=mount(NestedFrameExport,{target:document.body,props:{token:'token'}})
 try{await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(click).toHaveBeenCalledTimes(1));expect(nestedPlayback.playing).toBe(false);expect(exportNestedFramePng).toHaveBeenCalledWith(nestedComposition.document,1250000,{},'token',expect.any(AbortSignal),'root',{projectId:'saved',expectedRevision:3});expect(create).toHaveBeenCalledOnce();expect(revoke).toHaveBeenCalledWith('blob:frame')}finally{await unmount(component)}
})
it('aborts a late export on disposal without creating a download',async()=>{
 setup();let resolve!:(blob:Blob)=>void;vi.mocked(exportNestedFramePng).mockReturnValue(new Promise(done=>resolve=done));const create=vi.spyOn(URL,'createObjectURL')
 const component=mount(NestedFrameExport,{target:document.body,props:{token:'token'}});await tick();document.querySelector('button')!.click();await tick();const signal=vi.mocked(exportNestedFramePng).mock.calls[0][4]!
 await unmount(component);expect(signal.aborted).toBe(true);resolve(new Blob(['late']));await tick();expect(create).not.toHaveBeenCalled()
})
it('reports native errors and permits retry',async()=>{
 setup();vi.mocked(exportNestedFramePng).mockRejectedValue(Error('saved revision changed'));const component=mount(NestedFrameExport,{target:document.body,props:{token:'token'}})
 try{await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('saved revision changed'));expect(document.querySelector('button')!.disabled).toBe(false)}finally{await unmount(component)}
})
