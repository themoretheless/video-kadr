// @ts-expect-error Tests use the Svelte client mount entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Control from './NativeTemplateDownload.svelte'
it('downloads the saved template directly without fetching its binary',async()=>{
 const target=document.createElement('div');document.body.append(target);const urls:string[]=[]
 const click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(function(this:HTMLAnchorElement){urls.push(this.getAttribute('href')!)})
 const fetcher=vi.spyOn(globalThis,'fetch')
 const component=mount(Control,{target,props:{projectId:'saved/native',timeUs:500000,compositionId:'other/timeline'}})
 try{
  await tick();target.querySelector('button')!.click();await tick()
  expect(urls).toEqual(['/api/composition-projects/saved%2Fnative/template'])
  target.querySelector<HTMLInputElement>('input[type=checkbox]')!.click()
  await tick()
  target.querySelector('button')!.click()
  expect(urls[1]).toBe('/api/composition-projects/saved%2Fnative/template?posterTimeUs=500000&posterCompositionId=other%2Ftimeline')
  expect(fetcher).not.toHaveBeenCalled()
  expect(target.textContent).toContain('Загрузка шаблона сохранённой версии началась');expect(document.querySelector('a[href*="saved%2Fnative"]')).toBeNull()
 }finally{await unmount(component);target.remove();click.mockRestore();fetcher.mockRestore()}
})
it('disables unauthenticated or busy downloads',async()=>{
 const target=document.createElement('div');const click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(()=>{})
 const component=mount(Control,{target,props:{projectId:'saved',disabled:true}})
 try{await tick();expect(target.querySelector('button')!.disabled).toBe(true);target.querySelector('button')!.click();expect(click).not.toHaveBeenCalled()}
 finally{await unmount(component);click.mockRestore()}
})

it('saves the selected frame directly to the authenticated template library', async () => {
  const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
    new Response(JSON.stringify({ id: 'template', name: 'Design' }), { status: 201 }),
  )
  const onSaved = vi.fn()
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(Control, {
    target,
    props: { projectId: 'saved', token: 'session', timeUs: 500000, compositionId: 'root', onSaved },
  })
  try {
    await tick()
    target.querySelector<HTMLInputElement>('input')!.click()
    await tick()
    const save = [...target.querySelectorAll('button')].find(button =>
      button.textContent?.includes('Сохранить шаблон в библиотеку'),
    )!
    save.click()
    await vi.waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1))
    expect(fetcher).toHaveBeenCalledExactlyOnceWith(
      '/api/composition-projects/saved/template?posterTimeUs=500000&posterCompositionId=root',
      expect.objectContaining({ method: 'POST', headers: { Authorization: 'Bearer session' } }),
    )
    expect(target.textContent).toContain('Шаблон сохранён в библиотеку')
  } finally {
    await unmount(component)
    target.remove()
    fetcher.mockRestore()
  }
})
