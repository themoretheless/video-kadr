<script lang="ts">
 import NativeTemplatePoster from './NativeTemplatePoster.svelte'
 import {listNativeTemplateLibrary,installNativeTemplate,removeNativeTemplate,createProjectFromNativeLibrary,nativeTemplateLibraryUrl,type NativeTemplateLibraryEntry,type NativeTemplateImportResponse} from '$lib/api'
 import {doUpload} from '$lib/state/store.svelte'
 import {compositionSourceFromMediaInfo} from '$lib/state/composition/media.svelte'
 import type {ConcatImportSource} from '$lib/composition/concatProjectImport'
 let {refresh=0,token,spaceId,context,disabled=false,sources=[],oncreated}:{refresh?:number;token:string|null;spaceId?:string;context:unknown;disabled?:boolean;sources?:readonly ConcatImportSource[];oncreated:(result:NativeTemplateImportResponse,guard:()=>void)=>Promise<void>}=$props()
 let expanded=$state(false),entries=$state<NativeTemplateLibraryEntry[]>([]),selected=$state<string|null>(null),choices=$state<Record<string,string>>({}),file=$state<File|null>(null),busy=$state(false),error=$state(''),message=$state('')
 let projectName = $state('')
 let shared = $state(false)
 const catalogueSpace = $derived(shared ? spaceId : undefined)
 let uploadedSources = $state<ConcatImportSource[]>([])
 const availableSources = $derived([
  ...sources, ...uploadedSources.filter(item => !sources.some(source => source.source.id === item.source.id)),
 ])
 let epoch=0,controller:AbortController|undefined
 const entry=$derived(entries.find(item=>item.id===selected))
 const ready=$derived(!!entry&&entry.info.slots.every(slot=>availableSources.some(source=>source.source.id===choices[slot.mediaId])))
 $effect(()=>{
  const auth=token,open=expanded,scope=catalogueSpace;void spaceId;void context;void refresh
  epoch++;const serial=epoch;controller?.abort();entries=[];selected=null;choices={};uploadedSources=[];projectName='';file=null;error='';message='';busy=false
  if(!auth||!open)return
  const request=new AbortController();controller=request;busy=true
  void listNativeTemplateLibrary(auth,request.signal,scope).then(value=>{if(serial===epoch&&!request.signal.aborted)entries=value}).catch(reason=>{if(serial===epoch&&!request.signal.aborted)error=reason instanceof Error?reason.message:String(reason)}).finally(()=>{if(serial===epoch)busy=false})
  return()=>{epoch++;request.abort();controller?.abort()}
 })
 async function act(action:(auth:string,signal:AbortSignal,guard:()=>void)=>Promise<void>){
  if(!token||disabled||busy)return
  const serial=epoch,request=new AbortController();controller=request;busy=true;error='';message=''
  const guard=()=>{if(serial!==epoch||request.signal.aborted)throw new Error('Контекст библиотеки шаблонов изменился')}
  try{await action(token,request.signal,guard)}catch(reason){if(serial===epoch&&!request.signal.aborted)error=reason instanceof Error?reason.message:String(reason)}finally{if(serial===epoch)busy=false}
 }
 async function install(){const upload=file;if(!upload)return;await act(async(auth,signal,guard)=>{const saved=await installNativeTemplate(upload,auth,signal,catalogueSpace);guard();entries=[...entries,saved].sort((a,b)=>a.name.localeCompare(b.name));file=null;selected=saved.id;choices={};message='Шаблон добавлен в библиотеку'})}
 async function remove(id:string){await act(async(auth,signal,guard)=>{await removeNativeTemplate(id,auth,signal);guard();entries=entries.filter(entry=>entry.id!==id);if(selected===id){selected=null;choices={}}})}
 async function uploadSlot(mediaId: string, file: File | undefined) {
  const current = entry
  if (!file || current?.canManage === false || !current?.info.slots.some(slot => slot.mediaId === mediaId)) return
  await act(async (_auth, _signal, guard) => {
   const media = await doUpload(file, false)
   guard()
   if (selected !== current.id) return
   if (!media) throw new Error(`Не удалось загрузить ${file.name}`)
   const source = compositionSourceFromMediaInfo(media)
   uploadedSources = [...uploadedSources.filter(item => item.source.id !== source.id), { label: file.name, source }]
   choices = { ...choices, [mediaId]: source.id }
  })
 }
 async function create(){const current=entry;if(!current||current.canManage===false||!ready)return;await act(async(auth,signal,guard)=>{
  const fills=current.info.slots.map(slot=>({mediaId:slot.mediaId,sourceId:choices[slot.mediaId]!}))
  const saved=await createProjectFromNativeLibrary(current.id,fills,auth,spaceId,signal,projectName.trim() || undefined);guard();selected=null;choices={};message=`Создан проект «${saved.project.name}»`;await oncreated(saved,guard)
 })}
</script>
<details ontoggle={event=>{expanded=event.currentTarget.open}}>
 <summary>Библиотека шаблонов</summary>
 {#if !token}<p>Войдите, чтобы открыть свою библиотеку шаблонов.</p>{/if}
 {#if spaceId}
  <label>Каталог шаблонов
   <select aria-label="Каталог шаблонов" value={shared ? 'space' : 'personal'} onchange={event => shared = event.currentTarget.value === 'space'}>
    <option value="personal">Личный</option>
    <option value="space">Рабочее пространство</option>
   </select>
  </label>
 {/if}
 <label>Добавить пакет <input type="file" accept=".vktemplate" disabled={disabled||busy||!token} onchange={event=>{file=event.currentTarget.files?.[0]??null;event.currentTarget.value=''}}/></label>
 {#if file}<p>{file.name}</p>{/if}
 <button class="btn ghost sm" disabled={disabled||busy||!token||!file} onclick={()=>void install()}>Сохранить пакет в библиотеку</button>
 {#each entries as item(item.id)}
  <div>{#if item.hasPoster&&token}<NativeTemplatePoster id={item.id} name={item.name} {token}/>{/if}<button class="btn ghost sm" disabled={disabled||busy} onclick={()=>{selected=item.id;choices={};projectName='';error='';message=''}}>{item.name} · {item.info.width} × {item.info.height} · Слотов: {item.info.slots.length}</button>
   <a class="btn ghost sm" href={nativeTemplateLibraryUrl(item.id)} download>Скачать {item.name}</a>
   {#if item.canManage!==false}<button class="btn ghost sm danger" aria-label={`Удалить шаблон ${item.name}`} disabled={disabled||busy} onclick={()=>void remove(item.id)}>Удалить</button>{/if}
  </div>
 {:else}{#if token&&!busy}<p>Библиотека пока пуста.</p>{/if}{/each}
 {#if entry}
  <label>Название проекта <input type="text" bind:value={projectName} placeholder="Название шаблона" disabled={disabled || busy || entry.canManage === false} /></label>
  <p>Выбран шаблон «{entry.name}»</p>
  {#each entry.info.slots as slot(slot.mediaId)}
   <label>{slot.name} · {slot.seconds.toLocaleString('ru-RU',{maximumFractionDigits:3})} с
    <select aria-label={`Замена в библиотеке: ${slot.name}`} value={choices[slot.mediaId]??''} disabled={disabled||busy||entry.canManage===false} onchange={event=>{choices={...choices,[slot.mediaId]:event.currentTarget.value}}}>
     <option value="">Выберите файл из библиотеки</option>{#each availableSources as source(source.source.id)}<option value={source.source.id}>{source.label}</option>{/each}
    </select>
   </label>
   <label>Загрузить файл для слота {slot.name}
    <input type="file" accept="image/*,video/*,audio/*" disabled={disabled || busy || entry.canManage === false} onchange={event => {
     const selectedFile = event.currentTarget.files?.[0]
     event.currentTarget.value = ''
     void uploadSlot(slot.mediaId, selectedFile)
    }} />
   </label>
  {/each}
  <button class="btn primary sm" disabled={disabled||busy||!ready||entry.canManage===false} onclick={()=>void create()}>Создать проект из библиотечного шаблона</button>
 {/if}
 {#if busy}<p role="status">Обрабатываю шаблон…</p>{/if}
 {#if message}<p role="status">{message}</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
