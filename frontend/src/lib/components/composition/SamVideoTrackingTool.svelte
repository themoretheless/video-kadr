<script lang="ts">
 import SamModelManager from './SamModelManager.svelte'
 import {untrack} from 'svelte'
 import {getSamStatus,startSamVideoTracking,listSamVideoTracking,saveSamVideoRegion,cancelImageEnhancement,type CutoutSummary,type SavedSamVideoRegion} from '$lib/api'
 import {observeEnhancement} from '$lib/enhance-job-client'
 import {validateVideoRegion} from '$lib/video-region'
 import type {VideoMaskFrameRate} from '$lib/video-mask-clock'
 import type {Job} from '$lib/types'
 let{token,projectId,sourceId,sourceHash,targetKey,seedFrame,width,height,frames,frameRate,points,canAttach=true,disabled=false,onready,onbusy}:{token:string;projectId:string|null;sourceId:string|null;sourceHash:string|null;targetKey:string;seedFrame:number;width:number;height:number;frames:number;frameRate:VideoMaskFrameRate;points?:readonly [number,number][];canAttach?:boolean;disabled?:boolean;onready:(entry:SavedSamVideoRegion,jobId?:string)=>void|Promise<void>;onbusy:(busy:boolean)=>void}=$props()
 let available=$state<boolean|null>(null),busy=$state(false),cancelling=$state(false),jobId=$state(''),job=$state<Job|null>(null),error=$state(''),notice=$state('')
 let history=$state<CutoutSummary[]>([]),historyCursor=$state<string|null>(null),historyBusy=$state(false)
 let request:AbortController|undefined,historyRequest:AbortController|undefined,cancelRequest:AbortController|undefined
 const scope=$derived(JSON.stringify([token,projectId,sourceId,sourceHash,targetKey,seedFrame,width,height,frames,frameRate]))
 const active=$derived(Boolean(jobId)&&(!job||job.status==='pending'||job.status==='running'))
 const progress=$derived(typeof job?.progress==='number'&&Number.isFinite(job.progress)?Math.max(0,Math.min(100,job.progress)):undefined)
 const labels:Record<Job['status'],string>={pending:'Ожидает',running:'Выделяется объект',done:'Готово',cancelled:'Отменено',error:'Ошибка',interrupted:'Прервано'}
 function current(owner:string,controller:AbortController){return !controller.signal.aborted&&scope===owner}
 function message(reason:unknown){return reason instanceof Error?reason.message:String(reason)}
 $effect(()=>{const owner=scope;untrack(()=>{request?.abort();historyRequest?.abort();cancelRequest?.abort();available=null;busy=false;cancelling=false;jobId='';job=null;error='';notice='';history=[];historyCursor=null;historyBusy=false});return()=>{void owner;request?.abort();historyRequest?.abort();cancelRequest?.abort();untrack(()=>onbusy(false))}})
 $effect(()=>{const working=busy||active;untrack(()=>onbusy(working))})
 async function check(){if(disabled||busy||active)return;const captured=scope,controller=new AbortController();request?.abort();request=controller;busy=true;error='';try{const status=await getSamStatus(token,controller.signal);if(current(captured,controller))available=status.available&&status.video}catch(reason){if(current(captured,controller))error=message(reason)}finally{if(current(captured,controller))busy=false}}
 async function watch(id:string,controller:AbortController,captured:string){
  const target=projectId,source=sourceId,hash=sourceHash;if(!target||!source||!hash)throw new Error('Сохраните проект и откройте исходник маски')
  const record=(next:Job)=>{job=next;history=history.map(item=>item.jobId===id?{...item,status:next.status,progress:next.progress??null}:item)}
  const result=await observeEnhancement(token,id,controller.signal,next=>{if(current(captured,controller))record(next)});if(!current(captured,controller))return;record(result)
  if(result.status==='done'){
   const entry=await saveSamVideoRegion(token,id,target,controller.signal);if(!current(captured,controller))return
   validateVideoRegion(entry.snapshot)
   const r=entry.snapshot
   if(entry.source.id!==r.sourceId||entry.mask.id!==r.maskSourceId||entry.source.mediaType!=='video'||entry.mask.mediaType!=='video'||entry.mask.width!==256||entry.mask.height!==256||r.sourceHash!==hash||r.width!==width||r.height!==height||r.frames!==frames||r.frameRate.numerator!==frameRate.numerator||r.frameRate.denominator!==frameRate.denominator)throw new Error('Отслеженная область не соответствует исходнику')
   await onready(entry,id);if(!current(captured,controller))return;notice=`Область отслежена: ${r.trackedFrames} из ${r.frames} кадров.`

  }else if(result.status==='cancelled')notice='Выделение отменено'
  else throw new Error(result.error||'Не удалось выделить объект')
 }
 async function start(){if(disabled||busy||active||!available||!canAttach||!projectId||!sourceId||!sourceHash||!points?.length)return;const captured=scope,controller=new AbortController(),path=JSON.parse(JSON.stringify(points)) as [number,number][];request?.abort();request=controller;busy=true;job=null;jobId='';error='';notice='';try{const admitted=await startSamVideoTracking(token,sourceId,path,seedFrame,controller.signal);if(!current(captured,controller))return;if(!admitted.jobId)throw new Error('Сервер не вернул ID выделения');jobId=admitted.jobId;await watch(jobId,controller,captured)}catch(reason){if(current(captured,controller))error=message(reason)}finally{if(current(captured,controller))busy=false}}
 async function resume(id=jobId){if(disabled||busy||!id||!canAttach||!projectId||!sourceId||!sourceHash)return;const captured=scope,controller=new AbortController();request?.abort();request=controller;busy=true;jobId=id;job=null;error='';notice='';try{await watch(id,controller,captured)}catch(reason){if(current(captured,controller))error=message(reason)}finally{if(current(captured,controller))busy=false}}
 async function cancel(){if(!jobId||cancelling)return;const captured=scope,controller=new AbortController();cancelRequest?.abort();cancelRequest=controller;cancelling=true;try{await cancelImageEnhancement(token,jobId,controller.signal);if(current(captured,controller)){notice='Отмена выделения запрошена';if(!busy)await resume()}}catch(reason){if(current(captured,controller))error=message(reason)}finally{if(current(captured,controller))cancelling=false}}
 async function loadHistory(more=false){if(disabled||historyBusy||!sourceId)return;const captured=scope,source=sourceId,controller=new AbortController();historyRequest?.abort();historyRequest=controller;historyBusy=true;error='';try{const page=await listSamVideoTracking(token,source,more?historyCursor??undefined:undefined,controller.signal);if(current(captured,controller)){const items=page.jobs.filter(item=>item.sourceId===source);history=more?[...history,...items.filter(item=>!history.some(old=>old.jobId===item.jobId))]:items;historyCursor=page.nextCursor}}catch(reason){if(current(captured,controller))error=message(reason)}finally{if(current(captured,controller))historyBusy=false}}
</script>
<details><summary>Трекинг объекта SAM</summary>
 <p>Выберите исходный кадр и точки объекта. SAM отслеживает область вперёд и назад до потери объекта.</p>
 <SamModelManager {token} disabled={disabled||busy||active} onchanged={()=>{available=null}}/>
 <button type="button" class="btn ghost sm" disabled={disabled||busy||active} onclick={check}>Проверить SAM</button>
 {#if available===false}<p role="status">Модель SAM не настроена или недоступна.</p>{/if}
 {#if !projectId}<p>Сохраните проект, чтобы добавить область SAM.</p>{/if}
 <button type="button" class="btn ghost sm" disabled={disabled||busy||active||!available||!canAttach||!projectId||!sourceId||!sourceHash||!points?.length} onclick={start}>Отследить объект от выбранного кадра</button>
 {#if job}<p role="status">{labels[job.status]}</p>{/if}
 {#if busy||active}<progress aria-label="Трекинг объекта SAM" max="100" value={progress}></progress><button type="button" class="btn ghost sm" disabled={!jobId||cancelling} onclick={cancel}>Отменить трекинг SAM</button>{/if}
 {#if jobId&&!busy}<button type="button" class="btn ghost sm" disabled={disabled||!canAttach} onclick={()=>resume()}>Продолжить трекинг SAM</button>{/if}
 {#if notice}<p role="status">{notice}</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
 <details><summary>История областей SAM</summary><button type="button" class="btn ghost sm" disabled={disabled||historyBusy||!sourceId} onclick={()=>loadHistory()}>Обновить историю SAM</button><ul aria-label="История областей SAM">{#each history as item(item.jobId)}<li><button type="button" class="btn ghost sm" disabled={disabled||busy||active||!canAttach||!projectId||!sourceHash} onclick={()=>resume(item.jobId)}>{labels[item.status]} · {new Date(item.createdAt*1000).toLocaleString('ru-RU')}</button></li>{/each}</ul>{#if historyCursor}<button type="button" class="btn ghost sm" disabled={disabled||historyBusy} onclick={()=>loadHistory(true)}>Ещё области SAM</button>{/if}</details>
 <p>Закрытие панели не отменяет задание.</p>
</details>
<style>p{overflow-wrap:anywhere}</style>
