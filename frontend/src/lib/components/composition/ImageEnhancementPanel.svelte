<script lang="ts">
 import {untrack} from 'svelte'
 import EnhancementModelManager from './EnhancementModelManager.svelte'
 import {listImageEnhancements,type ImageEnhancementSummary,getImageEnhancementStatus,startImageEnhancement,getEnhancedImage,saveEnhancedImage,cancelImageEnhancement,type ImageEnhancementStatus} from '$lib/api'
 import {observeEnhancement} from '$lib/enhance-job-client'
 import type {Job,MediaEntry} from '$lib/types'
 let{token,projectId,sourceId,targetKey,width,height,context,onaccepted,disabled=false}:{token:string|null;projectId:string|null;sourceId:string;targetKey:string;width:number;height:number;context:object;onaccepted:(entry:MediaEntry)=>void|Promise<void>;disabled?:boolean}=$props()
 let status=$state<ImageEnhancementStatus|null>(null),factor=$state(2),busy=$state(false),applying=$state(false),cancelling=$state(false),error=$state(''),notice=$state(''),jobId=$state(''),job=$state<Job|null>(null),preview=$state('')
 let history=$state<ImageEnhancementSummary[]>([]),historyCursor=$state<string|null>(null),historyBusy=$state(false),historyError=$state('')
 let historyRequest:AbortController|undefined
 let observer:AbortController|undefined,cancelRequest:AbortController|undefined,ownedUrl=''
 const scope=$derived(JSON.stringify([token,projectId,sourceId,targetKey]))
 const active=$derived(Boolean(jobId)&&(!job||job.status==='pending'||job.status==='running'))
 const progress=$derived(typeof job?.progress==='number'&&Number.isFinite(job.progress)?Math.max(0,Math.min(100,job.progress)):undefined)
 const labels:Record<Job['status'],string>={pending:'Ожидает',running:'Улучшается',done:'Готово',cancelled:'Отменено',error:'Ошибка',interrupted:'Прервано'}
 function clearPreview(){if(ownedUrl)URL.revokeObjectURL(ownedUrl);ownedUrl='';preview=''}
 function current(controller:AbortController,captured:string){return !controller.signal.aborted&&scope===captured}
 function message(reason:unknown){return reason instanceof Error?reason.message:String(reason)}
 $effect(()=>{const captured=scope;untrack(()=>{observer?.abort();cancelRequest?.abort();historyRequest?.abort();history=[];historyCursor=null;historyBusy=false;historyError='';clearPreview();status=null;jobId='';job=null;busy=false;applying=false;cancelling=false;error='';notice='';factor=Math.max(width,height)>2048?1:2});return()=>{void captured;observer?.abort();cancelRequest?.abort();historyRequest?.abort();clearPreview()}})
 async function loadHistory(more=false){if(!token||historyBusy||disabled)return;const owner=token,captured=scope,controller=new AbortController();historyRequest?.abort();historyRequest=controller;historyBusy=true;historyError='';try{const page=await listImageEnhancements(owner,sourceId,more?historyCursor??undefined:undefined,controller.signal);if(current(controller,captured)){const items=page.jobs.filter(item=>item.sourceId===sourceId);history=more?[...history,...items.filter(item=>!history.some(old=>old.jobId===item.jobId))]:items;historyCursor=page.nextCursor}}catch(reason){if(current(controller,captured))historyError=message(reason)}finally{if(current(controller,captured))historyBusy=false}}
 async function openHistory(item:ImageEnhancementSummary){if(disabled||busy||applying||item.sourceId!==sourceId)return;jobId=item.jobId;job=null;notice='';clearPreview();await reconnect()}
 async function check(){if(disabled||!token||busy||applying||active)return;const captured=scope,owner=token,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';try{const result=await getImageEnhancementStatus(owner,controller.signal);if(current(controller,captured))status=result}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function watch(id:string,owner:string,controller:AbortController,captured:string){
  const result=await observeEnhancement(owner,id,controller.signal,next=>{if(current(controller,captured))job=next})
  if(!current(controller,captured))return;job=result
  if(result.status==='done'){const blob=await getEnhancedImage(owner,id,controller.signal);if(current(controller,captured)){clearPreview();ownedUrl=URL.createObjectURL(blob);preview=ownedUrl;notice='Копия готова. Примените её, чтобы сохранить оригинал для сравнения.'}}
  else if(result.status==='error'||result.status==='interrupted')throw new Error(result.error||'Не удалось улучшить изображение')
 }
 async function start(){if(disabled||!token||busy||active||applying||!status?.available)return;const captured=scope,owner=token,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';notice='';jobId='';job=null;clearPreview();try{const admitted=await startImageEnhancement(owner,sourceId,factor,controller.signal);if(!current(controller,captured))return;if(!admitted.jobId)throw new Error('Сервер не вернул ID улучшения');jobId=admitted.jobId;await watch(jobId,owner,controller,captured)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function reconnect(){if(!token||!jobId||busy||applying)return;const captured=scope,owner=token,id=jobId,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';try{await watch(id,owner,controller,captured)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function cancel(){if(!token||!jobId||cancelling)return;const captured=scope,owner=token,id=jobId,controller=new AbortController();cancelRequest?.abort();cancelRequest=controller;cancelling=true;error='';try{await cancelImageEnhancement(owner,id,controller.signal);if(current(controller,captured))notice='Отмена улучшения запрошена'}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))cancelling=false}}
 async function apply(){if(disabled||!token||!projectId||job?.status!=='done'||!preview||applying||busy)return;const captured=scope,original=context,owner=token,target=projectId,id=jobId,controller=new AbortController();observer?.abort();observer=controller;applying=true;error='';try{const entry=await saveEnhancedImage(owner,id,target,controller.signal);if(!current(controller,captured))return;if(context!==original){notice='Копия сохранена в медиатеке. Проект изменился; выберите её вручную.';return}await onaccepted(entry);notice='Обработанная копия применена. Оригинал сохранён для сравнения.'}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))applying=false}}
</script>
<details><summary>Улучшить изображение</summary>
 <p>Создайте отдельную улучшенную копию. Оригинал останется доступен для сравнения.</p>
 {#if !token}<p>Войдите, чтобы улучшить изображение.</p>{/if}
 {#if token}<EnhancementModelManager {token} disabled={disabled||busy||applying} onchanged={()=>{status=null}} />{/if}
 <button type="button" class="btn ghost sm" disabled={disabled||!token||busy||applying||active} onclick={check}>Проверить движок улучшения</button>
 {#if status&&!status.available}<p role="status">Модель улучшения не настроена или недоступна.</p>{/if}
 {#if status?.available}<label>Размер копии<select aria-label="Масштаб улучшения" value={factor} onchange={event=>{const next=Number(event.currentTarget.value);if(status?.factors.includes(next))factor=next}} disabled={disabled||busy||applying||active}>{#each status.factors as value}<option value={value}>{value}×</option>{/each}</select></label>{/if}
 <button type="button" class="btn ghost sm" disabled={disabled||!token||!status?.available||busy||active||applying} onclick={start}>Создать улучшенную копию</button>
 {#if job}<p role="status">{labels[job.status]}</p>{/if}
 {#if busy||active}<progress aria-label="Улучшение изображения" max="100" value={progress}></progress>{/if}
 {#if jobId&&(active||busy)}<button type="button" class="btn ghost sm" disabled={cancelling} onclick={cancel}>Отменить улучшение</button>{/if}
 {#if jobId&&!busy}<button type="button" class="btn ghost sm" disabled={applying} onclick={reconnect}>Обновить улучшение</button>{/if}
 {#if preview}<img class="enhancement-preview" src={preview} alt="Улучшенное изображение" /><button type="button" class="btn ghost sm" disabled={disabled||!projectId||busy||applying||job?.status!=='done'} onclick={apply}>Применить обработанную копию</button>{#if !projectId}<p>Сохраните проект, чтобы применить копию.</p>{/if}{/if}
 {#if applying}<p role="status">Сохраняю копию…</p>{/if}
 {#if notice}<p role="status">{notice}</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
 <details><summary>История улучшений этого изображения</summary><button type="button" class="btn ghost sm" disabled={disabled||!token||historyBusy} onclick={()=>loadHistory()}>Обновить историю улучшений</button><ul aria-label="История улучшений">{#each history as item (item.jobId)}<li><button type="button" class="btn ghost sm" disabled={disabled||busy||applying} onclick={()=>openHistory(item)}>{item.factor===null?'Масштаб не указан':`${item.factor}×`} · {labels[item.status]} · {new Date(item.createdAt*1000).toLocaleString('ru-RU')}</button></li>{/each}</ul>{#if historyCursor}<button type="button" class="btn ghost sm" disabled={disabled||historyBusy} onclick={()=>loadHistory(true)}>Ещё улучшения</button>{/if}{#if historyError}<p role="alert">{historyError}</p>{/if}</details>
 <p>Закрытие панели не отменяет фоновое задание.</p>
</details>
<style>.enhancement-preview{max-width:100%;height:auto}</style>
