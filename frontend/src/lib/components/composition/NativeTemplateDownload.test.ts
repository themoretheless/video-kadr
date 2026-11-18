// @ts-expect-error Tests use the Svelte client mount entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Control from './NativeTemplateDownload.svelte'
it('downloads the saved template directly without fetching its binary',async()=>{
 const target=document.createElement('div');document.body.append(target);const urls:string[]=[]
 const click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(function(this:HTMLAnchorElement){urls.push(this.getAttribute('href')!)})
 const fetcher=vi.spyOn(globalThis,'fetch')
 const component=mount(Control,{target,props:{projectId:'saved/native',timeUs:500000,compositionId:'other/timeline',hasSavedCover:true}})
 try{
  await tick();target.querySelector('button')!.click();await tick()
  expect(urls).toEqual(['/api/composition-projects/saved%2Fnative/template'])
  const cover = target.querySelector('select')!
  cover.selectedIndex = 1
  cover.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  target.querySelector('button')!.click()
  expect(urls[1]).toBe('/api/composition-projects/saved%2Fnative/template?posterTimeUs=500000&posterCompositionId=other%2Ftimeline')
  cover.selectedIndex = 2
  cover.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  target.querySelector('button')!.click()
  expect(urls[2]).toBe('/api/composition-projects/saved%2Fnative/template?useSavedCover=true')
  cover.value = 'auto'
  cover.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  target.querySelector('button')!.click()
  expect(urls[3]).toBe('/api/composition-projects/saved%2Fnative/template?autoPoster=true&posterCompositionId=other%2Ftimeline')
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

it.each(['frame', 'auto'])('saves the %s cover to the authenticated template library', async (source) => {
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
    const cover = target.querySelector('select')!
    cover.value = source
    cover.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const title = target.querySelector<HTMLInputElement>('input[type=text]')!
    title.value = 'Alternate design'
    title.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    const save = [...target.querySelectorAll('button')].find(button =>
      button.textContent?.includes('Сохранить шаблон в библиотеку'),
    )!
    save.click()
    await vi.waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1))
    expect(fetcher).toHaveBeenCalledExactlyOnceWith(
      `/api/composition-projects/saved/template?${source === 'frame' ? 'posterTimeUs=500000' : 'autoPoster=true'}&posterCompositionId=root&name=Alternate+design`,
      expect.objectContaining({ method: 'POST', headers: { Authorization: 'Bearer session' } }),
    )
    expect(target.textContent).toContain('Шаблон сохранён в библиотеку')
  } finally {
    await unmount(component)
    target.remove()
    fetcher.mockRestore()
  }
})

it('sends a chosen cover with a library save or a streaming download', async () => {
  const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
    new Response(JSON.stringify({ id: 'template' }), { status: 201 }),
  )
  const submit = vi.spyOn(HTMLFormElement.prototype, 'requestSubmit').mockImplementation(() => {})
  const onSaved = vi.fn()
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(Control, { target, props: { projectId: 'saved', token: 'session', onSaved } })
  try {
    await tick()
    const source = target.querySelector('select')!
    source.value = 'file'
    source.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const [download, save] = target.querySelectorAll('button')
    expect(download.disabled).toBe(true)
    expect(save.disabled).toBe(true)
    const file = new File(['cover'], 'cover.png', { type: 'image/png' })
    const input = target.querySelector<HTMLInputElement>('input[type=file]')!
    Object.defineProperty(input, 'files', { value: [file] })
    input.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(download.disabled).toBe(false)
    expect(save.disabled).toBe(false)
    download.click()
    expect(submit).toHaveBeenCalledOnce()
    expect(target.querySelector('form')?.getAttribute('action'))
      .toBe('/api/composition-projects/saved/template-cover?download=true')
    expect(fetcher).not.toHaveBeenCalled()
    save.click()
    await vi.waitFor(() => expect(onSaved).toHaveBeenCalledOnce())
    const [url, request] = fetcher.mock.calls[0]
    expect(url).toBe('/api/composition-projects/saved/template-cover')
    expect(request?.headers).toEqual({ Authorization: 'Bearer session' })
    expect((request?.body as FormData).get('poster')).toBe(file)
  } finally {
    await unmount(component)
    target.remove()
    fetcher.mockRestore()
    submit.mockRestore()
  }
})
