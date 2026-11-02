<script lang="ts">
 import { untrack } from 'svelte'
 import TtsModelDownloads from './TtsModelDownloads.svelte'
 import { listLocalTtsModels, selectLocalTtsModel, removeLocalTtsModel, type LocalTtsModelLibrary } from '$lib/api'
 let {token,disabled=false,onchanged}:{token:string;disabled?:boolean;onchanged:()=>void}=$props()
 let library=$state<LocalTtsModelLibrary|null>(null),selected=$state('configured'),pending=$state(false),error=$state(''),notice=$state('')
 let controller:AbortController|undefined
 const current=(owner:string,request:AbortController)=>owner===token&&!request.signal.aborted
 const message=(reason:unknown)=>reason instanceof Error?reason.message:String(reason)
 $effect(()=>{const owner=token;untrack(()=>{controller?.abort();library=null;selected='configured';pending=false;error='';notice=''});return()=>{void owner;controller?.abort()}})
 async function load(){if(pending||disabled)return;controller?.abort();const request=new AbortController(),owner=token;controller=request;pending=true;error='';notice='';try{const next=await listLocalTtsModels(owner,request.signal);if(current(owner,request)){library=next;selected=next.selectedId}}catch(reason){if(current(owner,request))error=message(reason)}finally{if(current(owner,request))pending=false}}
 async function choose(id:string){if(pending||disabled)return;const before=selected;selected=id;controller?.abort();const request=new AbortController(),owner=token;controller=request;pending=true;error='';notice='';try{const result=await selectLocalTtsModel(owner,id,request.signal);if(current(owner,request)){selected=result.selectedId;if(library)library={...library,selectedId:result.selectedId};notice='Выбор модели озвучивания сохранён';onchanged()}}catch(reason){if(current(owner,request)){selected=before;error=message(reason)}}finally{if(current(owner,request))pending=false}}
 async function remove(){if(pending||disabled||selected==='configured'||!library?.models.some(model=>model.id===selected))return;controller?.abort();const request=new AbortController(),owner=token,id=selected;controller=request;pending=true;error='';notice='';try{const result=await removeLocalTtsModel(owner,id,request.signal);if(!current(owner,request))return;onchanged();const next=await listLocalTtsModels(owner,request.signal);if(current(owner,request)){library=next;selected=next.selectedId;notice=result.fileReleased?'Модель озвучивания и её файлы удалены':'Модель удалена из вашей библиотеки. Общие файлы сохранены для других пользователей или принятых заданий.'}}catch(reason){if(current(owner,request))error=message(reason)}finally{if(current(owner,request))pending=false}}
</script>
<details><summary>Модели озвучивания</summary>
 <p>Скачайте локальную модель и выберите её для новых заданий. Выбор сохраняется для аккаунта; уже запущенная озвучка использует прежнюю модель. Для генерации нужен настроенный движок выбранной модели.</p>
 <button type="button" class="btn ghost sm" disabled={pending||disabled} onclick={load}>Обновить модели озвучивания</button>
 {#if library}<label>Модель для новых озвучек<select aria-label="Модель озвучивания" value={selected} disabled={pending||disabled} onchange={event=>void choose(event.currentTarget.value)}>
 <option value="configured">Серверная настройка по умолчанию</option>
 {#each library.models as model (model.id)}<option value={model.id}>{model.name} — {(model.bytes/1048576).toFixed(1)} МБ</option>{/each}
 </select></label><button type="button" class="btn ghost sm" disabled={pending||disabled||selected==='configured'} onclick={remove}>Удалить выбранную модель озвучивания</button>{/if}
 {#if pending}<p role="status">Проверяю модель озвучивания…</p>{/if}
 {#if notice}<p role="status">{notice}</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
 <TtsModelDownloads {token} {disabled} onready={()=>{void load()}} />
</details>
<style>
 label { display:flex; flex-direction:column; gap:0.25rem; margin-block:0.5rem; }
 select { max-width:100%; min-width:0; }
 p { overflow-wrap:anywhere; }
</style>
