// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Editor from './GraphicRollEditor.svelte'
it('authors every direction, entry and exit flags and precise pause/easing intervals',async()=>{
 const value={mode:'roll' as const,startOffScreen:true,endOffScreen:false,preroll:100000,easeIn:200000,easeOut:300000,postroll:400000}
 const target=document.createElement('div');document.body.append(target);const onchange=vi.fn()
 const component=mount(Editor,{target,props:{value,onchange}});await tick()
 const select=target.querySelector('select')!
 for(const mode of ['off','roll','crawlLeft','crawlRight']){select.value=mode;select.dispatchEvent(new Event('change',{bubbles:true}));expect(onchange).toHaveBeenLastCalledWith({...value,mode})}
 for(const key of ['preroll','easeIn','easeOut','postroll']){const input=target.querySelector<HTMLInputElement>(`[aria-label="Графика ${key}"]`)!;input.value='123456';input.dispatchEvent(new Event('change',{bubbles:true}));expect(onchange).toHaveBeenLastCalledWith({...value,[key]:123456})}
 for(const [label,key] of [['Графика вход за кадром','startOffScreen'],['Графика выход за кадр','endOffScreen']]){const input=target.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;input.checked=true;input.dispatchEvent(new Event('change',{bubbles:true}));expect(onchange).toHaveBeenLastCalledWith({...value,[key!]:true})}
 onchange.mockClear();const input=target.querySelector<HTMLInputElement>('[aria-label="Графика preroll"]')!;input.value='-1';input.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(onchange).not.toHaveBeenCalled();expect(target.querySelector('[role=alert]')).toBeTruthy()
 target.querySelector('button')!.click();expect(onchange).toHaveBeenLastCalledWith(undefined)
 await unmount(component);target.remove()
})
