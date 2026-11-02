<script lang="ts">
 import {untrack} from 'svelte'
 import MaskBrushEditor from './MaskBrushEditor.svelte'
 import CutoutModelManager from './CutoutModelManager.svelte'
 import {getSelfieStatus,startSelfie,getSelfieImage,saveSelfieImage,listSelfies,listCutouts,type CutoutSummary,getCutoutStatus,startCutout,getCutoutImage,saveCutoutImage,cancelImageEnhancement,type CutoutStatus,type CutoutAcceptedMedia} from '$lib/api'
 import {observeEnhancement} from '$lib/enhance-job-client'
 import type {Job,MediaEntry} from '$lib/types'
 let{token,projectId,projectMask,sourceId,targetKey,width,height,context,onaccepted,disabled=false}:{token:string|null;projectId:string|null;projectMask?:Omit<import('$lib/api').ProjectCutoutContext,'projectId'>;sourceId:string;targetKey:string;width:number;height:number;context:object;onaccepted:(entry:CutoutAcceptedMedia)=>void|Promise<void>;disabled?:boolean}=$props()
 let mode=$state<'object'|'selfie'>('object'),jobFamily=$state<'object'|'selfie'>('object')
 let brushDirty=$state(false)
 let status=$state<CutoutStatus|null>(null),busy=$state(false),applying=$state(false),cancelling=$state(false),error=$state(''),notice=$state(''),jobId=$state(''),job=$state<Job|null>(null),preview=$state('')
 let history=$state<(CutoutSummary&{family?:'selfie'})[]>([]),historyCursor=$state<string|null>(null),historyBusy=$state(false),historyError=$state('')
 let historySelfieCursor=$state<string|null>(null)
 let historyRequest:AbortController|undefined
 let observer:AbortController|undefined,cancelRequest:AbortController|undefined,ownedUrl=''
 const scope=$derived(JSON.stringify([token,projectId,sourceId,targetKey,projectMask,mode]))
 const sourceDigest=$derived(job?.result&&typeof job.result==='object'&&'sourceHash' in job.result&&typeof job.result.sourceHash==='string'?job.result.sourceHash:null)
 const active=$derived(Boolean(jobId)&&(!job||job.status==='pending'||job.status==='running'))
 const progress=$derived(typeof job?.progress==='number'&&Number.isFinite(job.progress)?Math.max(0,Math.min(100,job.progress)):undefined)
 const labels:Record<Job['status'],string>={pending:'Ожидает',running:'Удаляется фон',done:'Готово',cancelled:'Отменено',error:'Ошибка',interrupted:'Прервано'}
 function clearPreview(){if(ownedUrl)URL.revokeObjectURL(ownedUrl);ownedUrl='';preview=''}
 function current(controller:AbortController,captured:string){return !controller.signal.aborted&&scope===captured}
 function message(reason:unknown){return reason instanceof Error?reason.message:String(reason)}
 $effect(()=>{const captured=scope;untrack(()=>{observer?.abort();cancelRequest?.abort();historyRequest?.abort();history=[];historyCursor=null;historySelfieCursor=null;jobFamily='object';historyBusy=false;historyError='';clearPreview();status=null;jobId='';job=null;busy=false;applying=false;cancelling=false;error='';notice='';brushDirty=false});return()=>{void captured;observer?.abort();cancelRequest?.abort();historyRequest?.abort();clearPreview()}})
 async function loadHistory(more=false){if(!token||historyBusy||disabled)return;const owner=token,captured=scope,controller=new AbortController();historyRequest?.abort();historyRequest=controller;historyBusy=true;historyError='';try{const [page,selfies]=await Promise.all([more&&!historyCursor?Promise.resolve({jobs:[],nextCursor:null}):listCutouts(owner,sourceId,more?historyCursor??undefined:undefined,controller.signal),mode==='selfie'&&(!more||historySelfieCursor)?listSelfies(owner,sourceId,more?historySelfieCursor??undefined:undefined,controller.signal):Promise.resolve({jobs:[],nextCursor:null})]);if(current(controller,captured)){const items:(CutoutSummary&{family?:'selfie'})[]=[...page.jobs,...selfies.jobs.map(item=>({...item,family:'selfie' as const}))].filter(item=>item.sourceId===sourceId);history=more?[...history,...items.filter(item=>!history.some(old=>old.jobId===item.jobId))]:items;if(mode==='selfie')history.sort((a,b)=>b.createdAt-a.createdAt||b.jobId.localeCompare(a.jobId));historyCursor=page.nextCursor;historySelfieCursor=selfies.nextCursor}}catch(reason){if(current(controller,captured))historyError=message(reason)}finally{if(current(controller,captured))historyBusy=false}}

 async function openHistory(item:CutoutSummary&{family?:'selfie'}){if(disabled||busy||applying||brushDirty||item.sourceId!==sourceId)return;jobFamily=item.family??'object';jobId=item.jobId;job=null;notice='';clearPreview();await reconnect()}
 async function check(){if(disabled||!token||busy||applying||active)return;const captured=scope,owner=token,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';try{const result=await (mode==='selfie'?getSelfieStatus:getCutoutStatus)(owner,controller.signal);if(current(controller,captured))status=result}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function watch(id:string,owner:string,controller:AbortController,captured:string){
  function record(next:Job){job=next;history=history.map(item=>item.jobId===id?{...item,status:next.status,progress:next.progress??null}:item)}
  const family=jobFamily;const result=await observeEnhancement(owner,id,controller.signal,next=>{if(current(controller,captured))record(next)})
  if(!current(controller,captured))return;record(result)
  if(result.status==='done'){const blob=await (family==='selfie'?getSelfieImage:getCutoutImage)(owner,id,controller.signal);if(current(controller,captured)){clearPreview();ownedUrl=URL.createObjectURL(blob);preview=ownedUrl;notice='Копия готова. Примените её, чтобы сохранить оригинал для сравнения.'}}
  else if(result.status==='error'||result.status==='interrupted')throw new Error(result.error||'Не удалось удалить фон')
 }
 async function start(){if(disabled||!token||busy||active||applying||brushDirty||!status?.available)return;const captured=scope,owner=token,controller=new AbortController();observer?.abort();observer=controller;busy=true;jobFamily=mode;error='';notice='';jobId='';job=null;clearPreview();try{const admitted=await (mode==='selfie'?startSelfie:startCutout)(owner,sourceId,controller.signal);if(!current(controller,captured))return;if(!admitted.jobId)throw new Error('Сервер не вернул ID задания');jobId=admitted.jobId;await watch(jobId,owner,controller,captured)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function reconnect(){if(!token||!jobId||busy||applying)return;const captured=scope,owner=token,id=jobId,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';try{await watch(id,owner,controller,captured)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function cancel(){if(!token||!jobId||cancelling)return;const captured=scope,owner=token,id=jobId,controller=new AbortController();cancelRequest?.abort();cancelRequest=controller;cancelling=true;error='';try{await cancelImageEnhancement(owner,id,controller.signal);if(current(controller,captured))notice='Отмена удаления фона запрошена'}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))cancelling=false}}
 async function apply(){if(disabled||!token||!projectId||job?.status!=='done'||!preview||applying||busy||brushDirty)return;const captured=scope,original=context,owner=token,target=projectId,id=jobId,controller=new AbortController();observer?.abort();observer=controller;applying=true;error='';try{const entry=await (jobFamily==='selfie'?saveSelfieImage:saveCutoutImage)(owner,id,target,controller.signal);if(!current(controller,captured))return;if(context!==original){notice='Копия сохранена в медиатеке. Проект изменился; выберите её вручную.';return}await onaccepted(entry);notice='Обработанная копия применена. Оригинал сохранён для сравнения.'}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))applying=false}}
</script>
<details><summary>Удалить фон изображения</summary>
 <p>Создайте прозрачную копию без фона. Оригинал останется доступен для сравнения.</p>
 {#if !token}<p>Войдите, чтобы удалить фон изображения.</p>{/if}
 <label>Объект выделения<select aria-label="Модель удаления фона изображения" value={mode} disabled={disabled||busy||applying||active||brushDirty} onchange={event=>{const value=event.currentTarget.value;if(value==='object'||value==='selfie')mode=value}}><option value="object">Любой объект — IS-Net</option><option value="selfie">Человек — Selfie</option></select></label>
 {#if token&&mode==='object'}<CutoutModelManager {token} disabled={disabled||busy||applying} onchanged={()=>{status=null}} />{/if}
 <button type="button" class="btn ghost sm" disabled={disabled||!token||busy||applying||active} onclick={check}>Проверить удаление фона</button>
 {#if status&&!status.available}<p role="status">Модель удаления фона не настроена или недоступна.</p>{/if}
 <p>Размер изображения: {width} × {height}. Прозрачная копия сохранит этот размер.</p>
 <button type="button" class="btn ghost sm" disabled={disabled||!token||!status?.available||busy||active||applying||brushDirty} onclick={start}>Создать копию без фона</button>
 {#if job}<p role="status">{labels[job.status]}</p>{/if}
 {#if busy||active}<progress aria-label="Удаление фона изображения" max="100" value={progress}></progress>{/if}
 {#if jobId&&(active||busy)}<button type="button" class="btn ghost sm" disabled={cancelling} onclick={cancel}>Отменить удаление фона</button>{/if}
 {#if jobId&&!busy}<button type="button" class="btn ghost sm" disabled={applying} onclick={reconnect}>Обновить удаление фона</button>{/if}
 {#if preview}<img class="enhancement-preview" src={preview} alt="Изображение без фона" /><button type="button" class="btn ghost sm" disabled={disabled||!projectId||busy||applying||job?.status!=='done'||brushDirty} onclick={apply}>Применить обработанную копию</button>{#if !projectId}<p>Сохраните проект, чтобы применить копию.</p>{/if}{/if}
 {#if token&&preview&&job?.status==='done'}<MaskBrushEditor {token} {jobFamily} {jobId} {width} {height} {projectId} {sourceId} sourceHash={sourceDigest} disabled={disabled||busy||applying} onchanged={dirty=>{brushDirty=dirty}} onready={id=>{brushDirty=false;jobFamily='object';jobId=id;job=null;clearPreview();void reconnect()}} />{/if}
 {#if token&&projectId&&projectMask&&!preview&&!active}<p>Маска сохранена в проекте. Сохраните изменения проекта перед её пересчётом.</p><MaskBrushEditor {token} jobId="" {width} {height} projectMask={{...projectMask,projectId}} disabled={disabled||busy||applying} onchanged={dirty=>{brushDirty=dirty}} onready={id=>{brushDirty=false;jobFamily='object';jobId=id;job=null;clearPreview();void reconnect()}} />{/if}
 {#if brushDirty}<p role="status">Пересчитайте или отбросьте правки маски перед применением копии.</p>{/if}
 {#if applying}<p role="status">Сохраняю копию…</p>{/if}
 {#if notice}<p role="status">{notice}</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
 <details><summary>История удаления фона этого изображения</summary><button type="button" class="btn ghost sm" disabled={disabled||!token||historyBusy} onclick={()=>loadHistory()}>Обновить историю удаления фона</button><ul aria-label="История удаления фона">{#each history as item (item.jobId)}<li><button type="button" class="btn ghost sm" disabled={disabled||busy||applying||brushDirty} data-job-id={item.jobId} onclick={()=>openHistory(item)}>{labels[item.status]} · {new Date(item.createdAt*1000).toLocaleString('ru-RU')}</button></li>{/each}</ul>{#if historyCursor||historySelfieCursor}<button type="button" class="btn ghost sm" disabled={disabled||historyBusy} onclick={()=>loadHistory(true)}>Ещё задания</button>{/if}{#if historyError}<p role="alert">{historyError}</p>{/if}</details>
 <p>Закрытие панели не отменяет фоновое задание.</p>
</details>
<style>.enhancement-preview{display:block;width:320px;max-width:100%;height:auto;margin-block:.5rem;background-color:var(--surface);background-image:repeating-conic-gradient(var(--border) 0% 25%,transparent 0% 50%);background-size:20px 20px}</style>
