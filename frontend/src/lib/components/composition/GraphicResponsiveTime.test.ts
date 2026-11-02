// @ts-expect-error Client mount required under Vitest SSR conditions.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect,it,vi } from 'vitest'
import Editor from './GraphicResponsiveTime.svelte'
it('authors protected intervals, captures cursor boundaries and preserves signed design start',async()=>{
 const target=document.createElement('div');document.body.append(target)
 const value={intro:100000,outro:200000,designIn:-500000,designDuration:2000000}
 const onchange=vi.fn();const component=mount(Editor,{target,props:{value,duration:4000000,localUs:750000,onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Графика intro"]')!;input.value='300000';input.dispatchEvent(new Event('change',{bubbles:true}));expect(onchange).toHaveBeenLastCalledWith({...value,intro:300000})
 const buttons=[...target.querySelectorAll('button')];buttons.find(button=>button.textContent?.includes('Вступление'))!.click();expect(onchange).toHaveBeenLastCalledWith({...value,intro:750000})
 buttons.find(button=>button.textContent?.includes('Финал от'))!.click();expect(onchange).toHaveBeenLastCalledWith({...value,outro:3250000})
 onchange.mockClear();input.value='-1';input.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(onchange).not.toHaveBeenCalled();expect(target.querySelector('[role=alert]')).toBeTruthy()
 buttons.find(button=>button.textContent?.includes('Удалить'))!.click();expect(onchange).toHaveBeenLastCalledWith(undefined)
 await unmount(component);target.remove()
})
