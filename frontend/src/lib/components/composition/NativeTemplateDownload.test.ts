// @ts-expect-error Tests use the Svelte client mount entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Control from './NativeTemplateDownload.svelte'
it('downloads the saved template directly without fetching its binary',async()=>{
 const target=document.createElement('div');document.body.append(target);const urls:string[]=[]
 const click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(function(this:HTMLAnchorElement){urls.push(this.getAttribute('href')!)})
 const fetcher=vi.spyOn(globalThis,'fetch')
 const component=mount(Control,{target,props:{projectId:'saved/native'}})
 try{
  await tick();target.querySelector('button')!.click();await tick()
  expect(urls).toEqual(['/api/composition-projects/saved%2Fnative/template']);expect(fetcher).not.toHaveBeenCalled()
  expect(target.textContent).toContain('Загрузка шаблона сохранённой версии началась');expect(document.querySelector('a[href*="saved%2Fnative"]')).toBeNull()
 }finally{await unmount(component);target.remove();click.mockRestore();fetcher.mockRestore()}
})
it('disables unauthenticated or busy downloads',async()=>{
 const target=document.createElement('div');const click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(()=>{})
 const component=mount(Control,{target,props:{projectId:'saved',disabled:true}})
 try{await tick();expect(target.querySelector('button')!.disabled).toBe(true);target.querySelector('button')!.click();expect(click).not.toHaveBeenCalled()}
 finally{await unmount(component);click.mockRestore()}
})
