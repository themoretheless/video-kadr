// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {it,expect,vi} from 'vitest'
import {listUserGraphicTemplates,installUserGraphicTemplate,removeUserGraphicTemplate,instantiateGraphicTemplateFile} from '$lib/api'
import Library from './GraphicTemplateLibrary.svelte'
vi.mock('$lib/api',()=>({getGraphicTemplateThumbnail:vi.fn().mockResolvedValue({width:16,height:9,pngBase64:'AA=='}),getGraphicTemplateDocumentThumbnail:vi.fn().mockResolvedValue({width:16,height:9,pngBase64:'AA=='}),listUserGraphicTemplates:vi.fn(),installUserGraphicTemplate:vi.fn(),removeUserGraphicTemplate:vi.fn(),writeGraphicTemplateFile:vi.fn(),instantiateGraphicTemplateFile:vi.fn()}))
it('installs original contents and refreshes persisted catalog after removal',async()=>{
 const target=document.createElement('div');document.body.append(target)
 vi.mocked(listUserGraphicTemplates).mockResolvedValue([{file:'title.fcgt',document:{name:'My title'}}])
 vi.mocked(installUserGraphicTemplate).mockResolvedValue('title.fcgt')
 vi.mocked(removeUserGraphicTemplate).mockResolvedValue({removed:true})
 const onchoose=vi.fn();
 const component=mount(Library,{target,props:{onchoose}});await tick()
 const input=target.querySelector('input')!
 Object.defineProperty(input,'files',{value:[{name:'title.fcgt',text:async()=>'{"format":"filmcraft.graphicsTemplate"}'}]})
 input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(target.textContent).toContain('My title'))
 expect(installUserGraphicTemplate).toHaveBeenCalledWith('{"format":"filmcraft.graphicsTemplate"}')
 await tick()
 const template={id:'user:title',name:'Title'} as Awaited<ReturnType<typeof instantiateGraphicTemplateFile>>
 vi.mocked(instantiateGraphicTemplateFile).mockResolvedValue(template)
 target.querySelector<HTMLButtonElement>('[aria-label="Добавить title.fcgt"]')!.click()
 await vi.waitFor(()=>expect(onchoose).toHaveBeenCalledWith(template))
 expect(instantiateGraphicTemplateFile).toHaveBeenCalledWith({name:'My title'})
 await tick()
 vi.mocked(listUserGraphicTemplates).mockResolvedValue([])
 target.querySelector<HTMLButtonElement>('[aria-label="Удалить title.fcgt"]')!.click()
 await vi.waitFor(()=>expect(removeUserGraphicTemplate).toHaveBeenCalledWith('title.fcgt'))
 await vi.waitFor(()=>expect(target.textContent).not.toContain('My title'))
 await unmount(component);target.remove()
})

it('sends explicit replacement mode for a same-name installation',async()=>{
 vi.clearAllMocks()
 vi.mocked(listUserGraphicTemplates).mockResolvedValue([])
 vi.mocked(installUserGraphicTemplate).mockResolvedValue('title.fcgt')
 const target=document.createElement('div');document.body.append(target)
 const component=mount(Library,{target});await tick()
 const replace=target.querySelector<HTMLInputElement>('[aria-label="Заменить одноимённый шаблон"]')!
 replace.checked=true;replace.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const input=target.querySelector<HTMLInputElement>('input[type="file"]')!
 Object.defineProperty(input,'files',{value:[{name:'title.fcgt',text:async()=>'{"name":"Title"}'}]})
 input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(installUserGraphicTemplate).toHaveBeenCalledWith('{"name":"Title"}',undefined,{replace:true}))
 await vi.waitFor(()=>expect(listUserGraphicTemplates).toHaveBeenCalled())
 await unmount(component);target.remove()
})
