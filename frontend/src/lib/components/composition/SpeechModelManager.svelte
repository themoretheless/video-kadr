<script lang="ts">
 import { untrack } from 'svelte'
 import SpeechModelDownloads from './SpeechModelDownloads.svelte'
 import { listLocalSpeechModels,selectLocalSpeechModel,importLocalSpeechModel,removeLocalSpeechModel,type LocalSpeechModelLibrary } from '$lib/api'
 let {token,disabled=false,onchanged}:{token:string;disabled?:boolean;onchanged:()=>void}=$props()
 let library=$state<LocalSpeechModelLibrary|null>(null),selected=$state('configured'),file=$state<File|null>(null),pending=$state(false),uploading=$state(false),error=$state(''),notice=$state('')
 let fileInput:HTMLInputElement|undefined
 let controller:AbortController|undefined
 const current=(owner:string,request:AbortController)=>owner===token&&!request.signal.aborted
 const message=(reason:unknown)=>reason instanceof Error?reason.message:String(reason)
 $effect(()=>{const owner=token;untrack(()=>{controller?.abort();library=null;selected='configured';file=null;pending=false;uploading=false;error='';notice=''});return()=>{void owner;controller?.abort()}})
 async function load(){if(pending||disabled)return;controller?.abort();const request=new AbortController(),owner=token;controller=request;pending=true;error='';notice='';try{const next=await listLocalSpeechModels(owner,request.signal);if(current(owner,request)){library=next;selected=next.selectedId}}catch(reason){if(current(owner,request))error=message(reason)}finally{if(current(owner,request))pending=false}}
 async function choose(id:string){if(pending||disabled)return;const before=selected;selected=id;controller?.abort();const request=new AbortController(),owner=token;controller=request;pending=true;error='';notice='';try{const result=await selectLocalSpeechModel(owner,id,request.signal);if(current(owner,request)){selected=result.selectedId;if(library)library={...library,selectedId:result.selectedId};notice='Выбор модели сохранён';onchanged()}}catch(reason){if(current(owner,request)){selected=before;error=message(reason)}}finally{if(current(owner,request))pending=false}}
 async function install(){if(!file||pending||disabled)return;const selectedFile=file,owner=token;if(selectedFile.size<1_000_000||selectedFile.size>(library?.maxBytes??4_000_000_000)){error='Выберите модель Whisper размером от 1 МБ до 4 ГБ';return}controller?.abort();const request=new AbortController();controller=request;pending=true;uploading=true;error='';notice='';try{const imported=await importLocalSpeechModel(owner,selectedFile,request.signal);if(!current(owner,request))return;await selectLocalSpeechModel(owner,imported.id,request.signal);if(!current(owner,request))return;onchanged();const next=await listLocalSpeechModels(owner,request.signal);if(current(owner,request)){library=next;selected=next.selectedId;file=null;if(fileInput)fileInput.value='';notice='Модель импортирована и выбрана'}}catch(reason){if(current(owner,request))error=message(reason)}finally{if(current(owner,request)){pending=false;uploading=false}}}
 async function remove(){if(pending||disabled||selected==='configured'||!library?.models.some(model=>model.id===selected))return;controller?.abort();const request=new AbortController(),owner=token,id=selected;controller=request;pending=true;error='';notice='';try{const result=await removeLocalSpeechModel(owner,id,request.signal);if(!current(owner,request))return;onchanged();const next=await listLocalSpeechModels(owner,request.signal);if(current(owner,request)){library=next;selected=next.selectedId;notice=result.fileReleased?'Модель и её файл удалены':'Модель удалена из вашего списка. Общий файл пока сохранён для текущих задач или других пользователей.'}}catch(reason){if(current(owner,request))error=message(reason)}finally{if(current(owner,request))pending=false}}
 function cancel(){controller?.abort();pending=false;uploading=false;notice='Импорт остановлен. Обновите список моделей.'}
</script>
<details><summary>Модели распознавания речи</summary>
 <p>Импортируйте локальный файл модели Whisper в формате GGML (.bin). Выбор сохраняется для вашего аккаунта. Уже запущенные задания продолжат использовать прежнюю модель.</p>
 <button type="button" class="btn ghost sm" disabled={pending||disabled} onclick={load}>Обновить модели</button>
 {#if library}<label>Модель для новых заданий<select aria-label="Модель распознавания речи" value={selected} disabled={pending||disabled} onchange={event=>void choose(event.currentTarget.value)}>
  <option value="configured">{library.configured?.name??'Модель по умолчанию (не задана)'}</option>
  {#each library.models as model (model.id)}<option value={model.id} disabled={!model.available}>{model.name} — {(model.bytes/1048576).toFixed(1)} МБ{model.available?'':' (файл недоступен)'}{model.inUse?' (используется)':''}</option>{/each}
 </select></label><button type="button" class="btn ghost sm" disabled={pending||disabled||selected==='configured'} onclick={remove}>Удалить выбранную модель</button>{/if}
 <label>Файл модели<input bind:this={fileInput} type="file" accept=".bin" aria-label="Файл модели Whisper" disabled={pending||disabled} onchange={event=>{file=event.currentTarget.files?.[0]??null}} /></label>
 <button type="button" class="btn ghost sm" disabled={!file||pending||disabled} onclick={install}>Импортировать и выбрать</button>
 {#if uploading}<button type="button" class="btn ghost sm" onclick={cancel}>Остановить импорт</button><progress aria-label="Импорт модели"></progress>{/if}
 {#if pending}<p role="status">{uploading?'Импортирую модель…':'Обновляю настройки модели…'}</p>{/if}
 {#if notice}<p role="status">{notice}</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
 <SpeechModelDownloads {token} {disabled} onready={() => { onchanged(); void load() }} />
</details>
<style>
 label { display: flex; flex-direction: column; gap: 0.25rem; margin-block: 0.5rem; }
 select,input { max-width: 100%; min-width: 0; }
 p { overflow-wrap: anywhere; }
 progress { display: block; width: 100%; margin-block: 0.5rem; }
</style>
