// @ts-expect-error Mounted tests use Svelte's client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import Control from './NativeTemplateLibrary.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof api>(),listNativeTemplateLibrary:vi.fn(),installNativeTemplate:vi.fn(),removeNativeTemplate:vi.fn(),createProjectFromNativeLibrary:vi.fn()}))
const entry:api.NativeTemplateLibraryEntry={id:'stored',name:'Design',bytes:100,createdAt:1,info:{schemaVersion:1,compositionId:'root',width:16,height:8,slots:[{mediaId:'slot',name:'Picture',kind:'image',seconds:2}]}}
const sources=[{label:'Chosen',source:{id:'chosen',kind:'image' as const,durationTicks:0,width:16,height:8,hasAudio:false}}]
let component:ReturnType<typeof mount>|undefined
const button=(text:string)=>[...document.querySelectorAll('button')].find(b=>b.textContent?.includes(text))!
afterEach(async()=>{if(component)await unmount(component);component=undefined;vi.resetAllMocks();document.body.innerHTML=''})
async function setup(){const oncreated=vi.fn().mockResolvedValue(undefined);component=mount(Control,{target:document.body,props:{token:'owner',spaceId:'space',context:'document',sources,oncreated}});await tick();expect(api.listNativeTemplateLibrary).not.toHaveBeenCalled();const details=document.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(api.listNativeTemplateLibrary).toHaveBeenCalledTimes(1));await tick();return oncreated}
it('lists stored designs and creates from the chosen record without downloading or reuploading it',async()=>{
 vi.mocked(api.listNativeTemplateLibrary).mockResolvedValue([entry]);vi.mocked(api.createProjectFromNativeLibrary).mockResolvedValue({project:{id:'new',name:'Fresh'}} as api.NativeTemplateImportResponse)
 const oncreated=await setup();button('Design ·').click();await tick();expect(button('Создать проект из библиотечного шаблона').disabled).toBe(true)
 const select=document.querySelector<HTMLSelectElement>('[aria-label="Замена в библиотеке: Picture"]')!;select.value='chosen';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();const name = document.querySelector<HTMLInputElement>('input[type=text]')!
 name.value = ' Winter edit '
 name.dispatchEvent(new Event('input', { bubbles: true }))
 await tick()
 button('Создать проект из библиотечного шаблона').click()
 await vi.waitFor(()=>expect(oncreated).toHaveBeenCalledTimes(1));expect(api.createProjectFromNativeLibrary).toHaveBeenCalledWith('stored',[{mediaId:'slot',sourceId:'chosen'}],'owner','space',expect.any(AbortSignal),'Winter edit');expect(api.installNativeTemplate).not.toHaveBeenCalled();expect(document.body.textContent).toContain('Создан проект «Fresh»')
})
it('installs a selected file and removes the stored record through owner APIs',async()=>{
 vi.mocked(api.listNativeTemplateLibrary).mockResolvedValue([]);vi.mocked(api.installNativeTemplate).mockResolvedValue(entry);vi.mocked(api.removeNativeTemplate).mockResolvedValue(undefined)
 await setup();const input=document.querySelector('input')!,file=new File(['package'],'design.vktemplate');Object.defineProperty(input,'files',{value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}));await tick();button('Сохранить пакет в библиотеку').click()
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Шаблон добавлен'));expect(api.installNativeTemplate).toHaveBeenCalledWith(file,'owner',expect.any(AbortSignal),undefined);expect(document.querySelector('a')?.getAttribute('href')).toBe('/api/template-library/stored')
 document.querySelector<HTMLButtonElement>('button[aria-label="Удалить шаблон Design"]')!.click();await vi.waitFor(()=>expect(api.removeNativeTemplate).toHaveBeenCalledWith('stored','owner',expect.any(AbortSignal)));await vi.waitFor(()=>expect(document.querySelector('a')).toBeNull())
})
it('aborts library observation on collapse and ignores a late private list',async()=>{
 let resolve!:(value:api.NativeTemplateLibraryEntry[])=>void;vi.mocked(api.listNativeTemplateLibrary).mockImplementation(()=>new Promise(done=>{resolve=done}));await setup();const signal=vi.mocked(api.listNativeTemplateLibrary).mock.calls[0]![1]!
 const details=document.querySelector('details')!;details.open=false;details.dispatchEvent(new Event('toggle'));await tick();expect(signal.aborted).toBe(true);resolve([entry]);await tick();await Promise.resolve();expect(document.body.textContent).not.toContain('Design ·')
})

it('drops the previous owner list on account change before accepting a new one',async()=>{
 const{default:Harness}=await import('./fixtures/NativeTemplateLibraryHarness.svelte');const pending:Array<(entries:api.NativeTemplateLibraryEntry[])=>void>=[]
 vi.mocked(api.listNativeTemplateLibrary).mockImplementation(()=>new Promise(done=>pending.push(done)))
 const instance=mount(Harness,{target:document.body});component=instance;await tick();const details=document.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(pending).toHaveLength(1));const old=vi.mocked(api.listNativeTemplateLibrary).mock.calls[0]![1]!
 instance.changeAccount();await tick();await vi.waitFor(()=>expect(pending).toHaveLength(2));expect(old.aborted).toBe(true);pending[0]!([entry]);await tick();expect(document.body.textContent).not.toContain('Design ·');pending[1]!([]);await tick();expect(document.body.textContent).not.toContain('Design ·');expect(vi.mocked(api.listNativeTemplateLibrary).mock.calls[1]![0]).toBe('other')
})

it('renders an embedded poster in its library card through the owner-scoped loader',async()=>{
 vi.mocked(api.listNativeTemplateLibrary).mockResolvedValue([{...entry,hasPoster:true}])
 const poster=vi.spyOn(api,'getNativeTemplatePoster').mockResolvedValue(new Blob(['PNG'],{type:'image/png'}))
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:card-poster'),revoke=vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})
 try{await setup();await vi.waitFor(()=>expect(document.querySelector('img')?.getAttribute('src')).toBe('blob:card-poster'));expect(poster).toHaveBeenCalledWith('stored','owner',expect.any(AbortSignal));expect(document.querySelector('img')?.alt).toBe('Обложка Design')}
 finally{await unmount(component!);component=undefined;poster.mockRestore();create.mockRestore();revoke.mockRestore()}
})

it('uploads a local replacement and binds its registered source to the selected slot', async () => {
 vi.mocked(api.listNativeTemplateLibrary).mockResolvedValue([entry])
 vi.mocked(api.createProjectFromNativeLibrary).mockResolvedValue({project:{id:'new',name:'Fresh'}} as api.NativeTemplateImportResponse)
 const store = await import('$lib/state/store.svelte')
 const upload = vi.spyOn(store, 'doUpload').mockResolvedValue({
  id:'uploaded', filename:'photo.png', url:'/files/sources/photo.png', mediaType:'image', duration:0, width:16, height:8,
 })
 try {
  const oncreated = await setup()
  button('Design ·').click()
  await tick()
  const input = document.querySelector<HTMLInputElement>('input[accept^="image/"]')!
  const file = new File(['png'], 'photo.png', {type:'image/png'})
  Object.defineProperty(input, 'files', {value:[file]})
  input.dispatchEvent(new Event('change', {bubbles:true}))
  await vi.waitFor(() => expect(document.querySelector<HTMLSelectElement>('[aria-label="Замена в библиотеке: Picture"]')?.value).toBe('uploaded'))
  expect(upload).toHaveBeenCalledExactlyOnceWith(file, false)
  button('Создать проект из библиотечного шаблона').click()
  await vi.waitFor(() => expect(oncreated).toHaveBeenCalledOnce())
  expect(api.createProjectFromNativeLibrary).toHaveBeenCalledWith(
   'stored', [{mediaId:'slot', sourceId:'uploaded'}], 'owner', 'space', expect.any(AbortSignal), undefined,
  )
 } finally {
  upload.mockRestore()
 }
})


it('switches to the shared catalogue and presents viewer entries read-only', async () => {
 vi.mocked(api.listNativeTemplateLibrary)
  .mockResolvedValueOnce([entry])
  .mockResolvedValueOnce([{...entry, id: 'shared', name: 'Team design', spaceId: 'space', canManage: false}])
 await setup()
 await vi.waitFor(() => expect(button('Design ·')).toBeDefined())
 const selector = document.querySelector<HTMLSelectElement>('[aria-label="Каталог шаблонов"]')!
 selector.value = 'space'
 selector.dispatchEvent(new Event('change', {bubbles: true}))
 await vi.waitFor(() => expect(button('Team design ·')).toBeDefined())
 expect(api.listNativeTemplateLibrary).toHaveBeenLastCalledWith('owner', expect.any(AbortSignal), 'space')
 expect(button('Design ·')).toBeUndefined()
 expect(document.querySelector('button[aria-label="Удалить шаблон Team design"]')).toBeNull()
 expect(document.querySelector('a')?.getAttribute('href')).toBe('/api/template-library/shared')
 button('Team design ·').click()
 await tick()
 const slot = document.querySelector<HTMLSelectElement>('[aria-label="Замена в библиотеке: Picture"]')!
 slot.value = 'chosen'
 slot.dispatchEvent(new Event('change', {bubbles: true}))
 await tick()
 expect(button('Создать проект из библиотечного шаблона').disabled).toBe(true)
})
