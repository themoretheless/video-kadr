// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {it,expect,vi} from 'vitest'
import {getCompositionFonts,replaceCompositionFonts} from '$lib/api'
import {createComposition} from '$lib/composition/commands'
import Fonts from './CompositionFonts.svelte'
vi.mock('$lib/api',()=>({getCompositionFonts:vi.fn(),replaceCompositionFonts:vi.fn()}))
it('loads font inventory on demand even for an empty composition and cancels on close',async()=>{
 const composition=createComposition({width:80,height:64,fps:24,backgroundColor:'#000000'})
 vi.mocked(getCompositionFonts).mockResolvedValue([{family:'Inter',style:'Bold',count:3,missing:true}])
 const target=document.createElement('div');document.body.append(target)
 const component=mount(Fonts,{target,props:{composition}});await tick()
 expect(getCompositionFonts).not.toHaveBeenCalled()
 const details=target.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 await vi.waitFor(()=>expect(target.textContent).toContain('Inter · Bold ×3'))
 expect(target.querySelector('.missing')?.textContent).toBe('Не найден')
 const [wire,signal]=vi.mocked(getCompositionFonts).mock.calls[0]
 expect(wire.canvas.fpsMilli).toBe(24000);expect(wire.tracks).toEqual([])
 details.open=false;details.dispatchEvent(new Event('toggle'));await tick();expect(signal?.aborted).toBe(true)
 await unmount(component);target.remove()
})

it('submits selected face and commits returned font changes once',async()=>{
 const composition=createComposition({width:80,height:64,fps:24,backgroundColor:'#000000'})
 const {buildCompositionWireDocument}=await import('$lib/composition/payload')
 vi.mocked(getCompositionFonts).mockResolvedValue([{family:'Inter',style:'Bold',count:1}])
 vi.mocked(replaceCompositionFonts).mockResolvedValue({composition:buildCompositionWireDocument(composition),replaced:1})
 const target=document.createElement('div');document.body.append(target);const onchange=vi.fn()
 const component=mount(Fonts,{target,props:{composition,onchange}});await tick()
 const details=target.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 await vi.waitFor(()=>expect(target.querySelector('select option[value="Inter/Bold"]')).not.toBeNull());await tick()
 const select=target.querySelector('select')!;select.value='Inter/Bold';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const inputs=target.querySelectorAll('input');inputs[0].value='External';inputs[0].dispatchEvent(new Event('input',{bubbles:true}));inputs[1].value='Regular';inputs[1].dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(target.querySelector<HTMLButtonElement>('button')!.disabled,JSON.stringify({select:select.value,inputs:[...inputs].map(input=>input.value)})).toBe(false)
 target.querySelector('button')!.click()
 await vi.waitFor(()=>expect(onchange, target.textContent??'').toHaveBeenCalledTimes(1))
 expect(replaceCompositionFonts).toHaveBeenCalledWith(expect.anything(),{family:'Inter',style:'Bold'},{family:'External',style:'Regular'})
 expect(onchange).toHaveBeenCalledWith(composition)
 await unmount(component);target.remove()
})
