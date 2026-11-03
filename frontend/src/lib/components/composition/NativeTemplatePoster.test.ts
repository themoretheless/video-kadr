// @ts-expect-error Mounted tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import {getNativeTemplatePoster} from '$lib/api'
import Control from './NativeTemplatePoster.svelte'
vi.mock('$lib/api',()=>({getNativeTemplatePoster:vi.fn()}))
afterEach(()=>{vi.restoreAllMocks();vi.clearAllMocks();document.body.innerHTML=''})
it('loads a private PNG into an owned blob URL and releases it on unmount',async()=>{
 const blob=new Blob(['PNG'],{type:'image/png'});vi.mocked(getNativeTemplatePoster).mockResolvedValue(blob)
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:private-template'),revoke=vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})
 const component=mount(Control,{target:document.body,props:{id:'saved',name:'Edit',token:'owner'}})
 await vi.waitFor(()=>expect(document.querySelector('img')?.getAttribute('src')).toBe('blob:private-template'));expect(create).toHaveBeenCalledWith(blob);expect(document.querySelector('img')?.alt).toBe('Обложка Edit')
 const signal=vi.mocked(getNativeTemplatePoster).mock.calls[0]![2]!;await unmount(component);expect(signal.aborted).toBe(true);expect(revoke).toHaveBeenCalledWith('blob:private-template')
})
it('does not create a URL for a late response after disposal',async()=>{
 let resolve!:(blob:Blob)=>void;vi.mocked(getNativeTemplatePoster).mockImplementation(()=>new Promise(done=>{resolve=done}));const create=vi.spyOn(URL,'createObjectURL')
 const component=mount(Control,{target:document.body,props:{id:'saved',name:'Edit',token:'owner'}});await tick();await unmount(component);resolve(new Blob(['PNG'],{type:'image/png'}));await tick();await Promise.resolve();expect(create).not.toHaveBeenCalled()
})
