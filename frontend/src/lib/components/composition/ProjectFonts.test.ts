// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import {getProjectFonts,replaceProjectFonts,type NativeNestedProject} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition} from '$lib/state/nestedComposition.svelte'
import ProjectFonts from './ProjectFonts.svelte'
vi.mock('$lib/api',()=>({getProjectFonts:vi.fn(),replaceProjectFonts:vi.fn()}))
it('submits whole-project face replacement through shared history and restores render bindings',async()=>{
 closeNestedComposition()
 const before:NativeNestedProject={schemaVersion:1,root:'root',compositions:{}}
 const after:NativeNestedProject={...before,luts:[]}
 nestedComposition.document=before;nestedComposition.renderedOutputs={root:'old-render'}
 vi.mocked(getProjectFonts).mockImplementation(async project=>project.luts?[{family:'External',style:'Bold',count:4}]:[{family:'Inter',style:'Bold',count:4}])
 vi.mocked(replaceProjectFonts).mockResolvedValue({project:after,replaced:4})
 const target=document.createElement('div');document.body.append(target)
 const component=mount(ProjectFonts,{target});await tick()
 const details=target.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 await vi.waitFor(()=>expect(target.querySelector('select')).not.toBeNull());await tick()
 const select=target.querySelector('select')!;select.value='Inter/Bold';select.dispatchEvent(new Event('change',{bubbles:true}))
 const input=target.querySelector<HTMLInputElement>('input[aria-label="Новое семейство шрифта проекта"]')!;input.value='External';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 target.querySelector('button')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 expect(replaceProjectFonts).toHaveBeenCalledWith(before,{family:'Inter',style:'Bold'},{family:'External'})
 expect(nestedComposition.document).toEqual(after);expect(nestedComposition.renderedOutputs).toEqual({})
 await vi.waitFor(()=>{expect(target.querySelector('option[value="External/Bold"]')).not.toBeNull();expect(target.querySelector('select')?.value).toBe('')});expect(target.querySelector('button')!.disabled).toBe(true)
 undoNestedComposition();expect(nestedComposition.document).toEqual(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 await unmount(component);target.remove();closeNestedComposition()
})
