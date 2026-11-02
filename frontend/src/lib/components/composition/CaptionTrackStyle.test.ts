// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Style from './CaptionTrackStyle.svelte'
it('exposes caption style fields and retains RGBA alpha when changing a numeric field',async()=>{
 const target=document.createElement('div'),onchange=vi.fn()
 const style={color:[171,205,239,128] as const,size:42,outline:2}
 const component=mount(Style,{target,props:{style,onchange}});await tick()
 expect(target.querySelector<HTMLInputElement>('[aria-label="Цвет субтитров"]')!.value).toBe('#abcdef80')
 const size=target.querySelector<HTMLInputElement>('[aria-label="Размер субтитров"]')!;size.value='64';size.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(onchange).toHaveBeenLastCalledWith({...style,size:64})
 const anchor=target.querySelector<HTMLSelectElement>('[aria-label="Положение субтитров"]')!;anchor.value='top';anchor.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(onchange).toHaveBeenLastCalledWith({...style,anchor:'top'})
 expect(target.querySelectorAll('input,select')).toHaveLength(11)
 await unmount(component)
})
