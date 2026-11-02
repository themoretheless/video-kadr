<script lang="ts">
 import {isVideoMaskFrameRate,isVideoMaskNavigation,type VideoMaskNavigation} from '$lib/video-mask-clock'
 import {tick,untrack} from 'svelte'
 import VideoMaskBrushEditor from './VideoMaskBrushEditor.svelte'
 import {continueVideoWithRegion,getVideoCorrectionRecipe,startProjectVideoCorrection,type ProjectVideoMaskContext,listPersonVideos,type CutoutSummary,getPersonVideoStatus,startPersonVideo,savePersonVideo,cancelImageEnhancement,type PersonVideoStatus,type SavedPersonVideo} from '$lib/api'
 import {observeEnhancement} from '$lib/enhance-job-client'
 import type {Job} from '$lib/types'
 let{token,projectId,sourceId,targetKey,width,height,context,onaccepted,savedMask,ontracked,disabled=false}:{token:string|null;projectId:string|null;sourceId:string;targetKey:string;width:number;height:number;context:object;onaccepted:(entry:SavedPersonVideo)=>void|Promise<void>;savedMask?:ProjectVideoMaskContext;ontracked?:(entry:import('$lib/api').SavedSamVideoRegion,expected:import('$lib/composition/types').VideoMaskAttachment)=>Promise<void>;disabled?:boolean}=$props()
 let status=$state<PersonVideoStatus|null>(null),busy=$state(false),applying=$state(false),cancelling=$state(false),error=$state(''),notice=$state(''),jobId=$state(''),job=$state<Job|null>(null),ready=$state(false)
 let alive=true,handoffRequest:AbortController|undefined
 let continuing=$state(false),continuationId=$state(''),continuationSam=$state(''),continuationProgress=$state<number|undefined>(undefined),brushAutoOpen=$state(false),preferredRegionId=$state(''),preferredFrame=$state(0),preferredView=$state<VideoMaskNavigation['view']>('mask')
 let brushDirty=$state(false),savedMode=$state(false),historyMode=$state(false)
 type BrushSession={id:string;saved:boolean;history:boolean;region:string;navigation:VideoMaskNavigation}
 const brushSessions=new Map<string,BrushSession>()
 let sessionIdentity='',lastResetScope='',displayedScope=''
 function rememberBrush(key:string,navigation?:VideoMaskNavigation,region=preferredRegionId){
  if(!ready||job?.status!=='done'||!jobId)return
  const position=isVideoMaskNavigation(navigation,frameCount)?navigation:{frame:preferredFrame,view:preferredView}
  brushSessions.delete(key);brushSessions.set(key,{id:jobId,saved:savedMode,history:historyMode,region,navigation:position})
  if(brushSessions.size>32)brushSessions.delete(brushSessions.keys().next().value!)
 }

 const videoResult=$derived(job?.result&&typeof job.result==='object'&&!Array.isArray(job.result)?job.result as Record<string,unknown>:null)
 const frameRate=$derived(isVideoMaskFrameRate(videoResult?.frameRate)?videoResult.frameRate:undefined)
 const frameCount=$derived(typeof videoResult?.frames==='number'&&Number.isSafeInteger(videoResult.frames)&&videoResult.frames>0&&videoResult.frames<=360000?videoResult.frames:0)
 const frameWidth=$derived(typeof videoResult?.width==='number'&&Number.isSafeInteger(videoResult.width)&&videoResult.width>0?videoResult.width:width)
 const frameHeight=$derived(typeof videoResult?.height==='number'&&Number.isSafeInteger(videoResult.height)&&videoResult.height>0?videoResult.height:height)
 let history=$state<CutoutSummary[]>([]),historyCursor=$state<string|null>(null),historyBusy=$state(false),historyError=$state('')
 let historyRequest:AbortController|undefined
 let observer:AbortController|undefined,cancelRequest:AbortController|undefined
 const identity=$derived(JSON.stringify([token,projectId,sourceId,targetKey]))
 const scope=$derived(JSON.stringify([token,projectId,sourceId,targetKey,savedMask]))
 const historySource=$derived(savedMask?.snapshot.sourceId??sourceId)
 const supported=$derived(Boolean(status?.available&&status.video))
 const active=$derived(Boolean(jobId)&&(!job||job.status==='pending'||job.status==='running'))
 const progress=$derived(typeof job?.progress==='number'&&Number.isFinite(job.progress)?Math.max(0,Math.min(100,job.progress)):undefined)
 const labels:Record<Job['status'],string>={pending:'Ожидает',running:'Удаляется фон',done:'Готово',cancelled:'Отменено',error:'Ошибка',interrupted:'Прервано'}
 $effect(()=>()=>{alive=false;observer?.abort();cancelRequest?.abort();historyRequest?.abort();handoffRequest?.abort();brushSessions.clear()})
 function clearPreview(){ready=false}
 function current(controller:AbortController,captured:string){return !controller.signal.aborted&&scope===captured}
 function message(reason:unknown){return reason instanceof Error?reason.message:String(reason)}
 $effect(()=>{const captured=scope,target=identity;if(lastResetScope===captured)return;lastResetScope=captured;untrack(()=>{if(sessionIdentity!==target){brushSessions.clear();sessionIdentity=target}const retained=brushSessions.get(captured);displayedScope='';observer?.abort();cancelRequest?.abort();historyRequest?.abort();handoffRequest?.abort();continuing=false;continuationId='';continuationSam='';continuationProgress=undefined;brushAutoOpen=false;preferredRegionId='';preferredFrame=0;preferredView='mask';brushDirty=false;savedMode=false;historyMode=false;history=[];historyCursor=null;historyBusy=false;historyError='';clearPreview();status=null;jobId='';job=null;busy=false;applying=false;cancelling=false;error='';notice='';if(retained){jobId=retained.id;savedMode=retained.saved;historyMode=retained.history;preferredRegionId=retained.region;preferredFrame=retained.navigation.frame;preferredView=retained.navigation.view;brushAutoOpen=true;queueMicrotask(()=>{if(alive&&scope===captured&&jobId===retained.id)void reconnect()})}})})
 async function tracked(entry:import('$lib/api').SavedSamVideoRegion,expected:import('$lib/composition/types').VideoMaskAttachment,samJobId?:string,corrections?:import('$lib/api').VideoFrameCorrection[],navigation?:VideoMaskNavigation){
  if(disabled||!token||!projectId||!ontracked||!samJobId||!corrections||continuing)throw new Error('Не удалось продолжить правку новой области')
  const captured=scope,target=identity,original=context,owner=token,parent=jobId,controller=new AbortController()
  handoffRequest?.abort();handoffRequest=controller;continuing=true;continuationProgress=undefined
  try{
   if(continuationSam!==samJobId||!continuationId){const admitted=await continueVideoWithRegion(owner,parent,samJobId,corrections,controller.signal);if(!current(controller,captured)||context!==original)throw new Error('Выбор клипа изменился');if(!admitted.jobId)throw new Error('Сервер не вернул ID подготовки области');continuationSam=samJobId;continuationId=admitted.jobId}
   const id=continuationId,result=await observeEnhancement(owner,id,controller.signal,next=>{if(current(controller,captured))continuationProgress=next.progress??undefined})
   if(!current(controller,captured)||context!==original||disabled)throw new Error('Выбор клипа изменился')
   if(result.status!=='done'){if(result.status==='cancelled'){continuationId='';continuationSam=''}throw new Error(result.error||'Подготовка области не завершена')}
   rememberBrush(captured,navigation)
   await ontracked(entry,expected)
   // The inventory transaction intentionally changes scope. Let that reset settle,
   // then adopt only if this same live clip now owns the exact expected inventory.
   await tick()
   const regions=expected.trackedRegions??[],retained=regions.some(r=>r.maskSourceId===entry.snapshot.maskSourceId)?regions:[...regions,entry.snapshot]
   const expectedAfter={...expected,trackedRegions:retained}
   if(!alive||identity!==target||disabled||JSON.stringify(savedMask?.snapshot)!==JSON.stringify(expectedAfter))throw new Error('Проект изменился; откройте видеомаски заново')
   jobId=id;job=result;ready=true;displayedScope=scope;savedMode=true;brushDirty=false;brushAutoOpen=true;preferredRegionId=entry.snapshot.maskSourceId;preferredFrame=isVideoMaskNavigation(navigation,entry.snapshot.frames)?navigation.frame:entry.snapshot.seedFrame;preferredView=isVideoMaskNavigation(navigation,entry.snapshot.frames)?navigation.view:'mask';notice='Новая область готова к правке.';rememberBrush(scope)
  }finally{if(alive&&!controller.signal.aborted&&scope===captured&&handoffRequest===controller)continuing=false}
 }
 function navigateBrush(id:string,navigation:VideoMaskNavigation,region:string){
  if(displayedScope!==scope||!ready||job?.status!=='done'||id!==jobId||!isVideoMaskNavigation(navigation,frameCount))return
  rememberBrush(scope,navigation,region)
 }
 function corrected(id:string,navigation?:VideoMaskNavigation){
  const retained=brushSessions.get(scope);if(retained)preferredRegionId=retained.region
  if(isVideoMaskNavigation(navigation,frameCount)){preferredFrame=navigation.frame;preferredView=navigation.view}
  brushAutoOpen=true;brushDirty=false;jobId=id;job=null;clearPreview();void reconnect()
 }
 async function cancelContinuation(){if(!token||!continuationId||cancelling)return;const owner=token,id=continuationId,captured=scope,controller=new AbortController();cancelRequest?.abort();cancelRequest=controller;cancelling=true;try{await cancelImageEnhancement(owner,id,controller.signal)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))cancelling=false}}
 async function loadHistory(more=false){if(!token||historyBusy||disabled)return;const owner=token,captured=scope,controller=new AbortController();historyRequest?.abort();historyRequest=controller;historyBusy=true;historyError='';try{const page=await listPersonVideos(owner,historySource,more?historyCursor??undefined:undefined,controller.signal);if(current(controller,captured)){const items=page.jobs.filter(item=>item.sourceId===historySource);history=more?[...history,...items.filter(item=>!history.some(old=>old.jobId===item.jobId))]:items;historyCursor=page.nextCursor}}catch(reason){if(current(controller,captured))historyError=message(reason)}finally{if(current(controller,captured))historyBusy=false}}
 async function openHistory(item:CutoutSummary){if(disabled||brushDirty||busy||applying||item.sourceId!==historySource)return;jobId=item.jobId;job=null;historyMode=true;notice='';clearPreview();await reconnect()}
 async function check(){if(disabled||brushDirty||!token||busy||applying||active)return;const captured=scope,owner=token,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';try{const result=await getPersonVideoStatus(owner,controller.signal);if(current(controller,captured))status=result}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function watch(id:string,owner:string,controller:AbortController,captured:string){
  const result=await observeEnhancement(owner,id,controller.signal,next=>{if(current(controller,captured))job=next})
  if(!current(controller,captured))return;job=result
  if(result.status==='done'&&historyMode&&savedMask&&projectId){
   const recipe=await getVideoCorrectionRecipe(owner,id,controller.signal)
   if(!current(controller,captured))return
   const origin=recipe.projectOrigin
   if(origin){
    const stable=(value:unknown):string=>JSON.stringify(value,(_key,item)=>item&&typeof item==='object'&&!Array.isArray(item)?Object.fromEntries(Object.entries(item).sort(([a],[b])=>a.localeCompare(b))):item)
    const expected=stable(savedMask.snapshot)
    if(origin.projectId!==projectId||origin.clipId!==savedMask.clipId||(origin.compositionId??undefined)!==savedMask.compositionId||(stable(origin.snapshot)!==expected&&(!origin.savedSnapshot||stable(origin.savedSnapshot)!==expected)))throw new Error('Эта видеомаска относится к другому проекту, клипу или снимку')
    savedMode=true
   }
  }
  if(result.status==='done'){ready=true;displayedScope=captured;rememberBrush(captured);notice=savedMode?'Сохранённые видеомаски готовы к правке.':'Видеокопия готова. Примените её, чтобы сохранить оригинал для сравнения.'}
  else if(result.status==='error'||result.status==='interrupted')throw new Error(result.error||'Не удалось удалить фон видео')
 }
 async function start(){if(disabled||brushDirty||!token||busy||active||applying||!supported)return;const captured=scope,owner=token,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';notice='';jobId='';job=null;savedMode=false;historyMode=false;clearPreview();try{const admitted=await startPersonVideo(owner,sourceId,controller.signal);if(!current(controller,captured))return;if(!admitted.jobId)throw new Error('Сервер не вернул ID удаления фона');jobId=admitted.jobId;await watch(jobId,owner,controller,captured)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function openSaved(){
  if(disabled||brushDirty||!token||!projectId||!savedMask||busy||active||applying)return
  const captured=scope,owner=token,target=projectId,original=context,saved=savedMask,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';notice='Подготавливаю сохранённые видеомаски…';jobId='';job=null;savedMode=true;historyMode=false;clearPreview()
  try{const admitted=await startProjectVideoCorrection(owner,target,saved,controller.signal);if(!current(controller,captured))return;if(context!==original){notice='Проект изменился. Откройте видеомаски заново.';return}if(!admitted.jobId)throw new Error('Сервер не вернул ID подготовки видеомасок');jobId=admitted.jobId;await watch(jobId,owner,controller,captured)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}
 }
 async function reconnect(){if(brushDirty||!token||!jobId||busy||applying)return;const captured=scope,owner=token,id=jobId,controller=new AbortController();observer?.abort();observer=controller;busy=true;error='';try{await watch(id,owner,controller,captured)}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))busy=false}}
 async function cancel(){if(!token||!jobId||cancelling)return;const captured=scope,owner=token,id=jobId,controller=new AbortController();cancelRequest?.abort();cancelRequest=controller;cancelling=true;error='';try{await cancelImageEnhancement(owner,id,controller.signal);if(current(controller,captured))notice='Отмена удаления фона запрошена'}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))cancelling=false}}
 async function apply(){if(disabled||brushDirty||!token||!projectId||job?.status!=='done'||!ready||applying||busy)return;const captured=scope,original=context,owner=token,target=projectId,id=jobId,controller=new AbortController();observer?.abort();observer=controller;applying=true;error='';try{const entry=await savePersonVideo(owner,id,target,controller.signal);if(!current(controller,captured))return;if(context!==original){notice='Копия сохранена в медиатеке. Проект изменился; выберите её вручную.';return}await onaccepted(entry);notice='Обработанная копия применена. Оригинал сохранён для сравнения.'}catch(reason){if(current(controller,captured))error=message(reason)}finally{if(current(controller,captured))applying=false}}
</script>
<details><summary>Удалить фон видео</summary>
 <p>Отделите человека от фона. Оригинал останется доступен для сравнения. Поддерживается видео SDR до 8 бит с постоянной частотой кадров.</p>
 {#if savedMask}<p>Можно открыть сохранённые видеомаски без повторного удаления фона. Сначала сохраните изменения проекта.</p><button type="button" class="btn ghost sm" disabled={disabled||brushDirty||!token||!projectId||busy||active||applying} onclick={openSaved}>Открыть сохранённые видеомаски</button>{/if}
 {#if !token}<p>Войдите, чтобы удалить фон видео.</p>{/if}
 <button type="button" class="btn ghost sm" disabled={disabled||brushDirty||!token||busy||applying||active} onclick={check}>Проверить удаление фона</button>
 {#if status&&!supported}<p role="status">Модель удаления фона не настроена или недоступна.</p>{/if}
 <button type="button" class="btn ghost sm" disabled={disabled||brushDirty||!token||!supported||busy||active||applying} onclick={start}>Создать видео без фона</button>
 {#if job}<p role="status">{savedMode&&job.status==='running'?'Подготавливаются видеомаски':labels[job.status]}</p>{/if}
 {#if busy||active}<progress aria-label="Удаление фона видео" max="100" value={progress}></progress>{/if}
 {#if jobId&&(active||busy)}<button type="button" class="btn ghost sm" disabled={cancelling} onclick={cancel}>Отменить удаление фона</button>{/if}
 {#if jobId&&!busy}<button type="button" class="btn ghost sm" disabled={brushDirty||applying} onclick={reconnect}>Обновить удаление фона</button>{/if}
 {#if ready}<p>Видеокопия сохраняет исходный звук. После применения просмотрите её в таймлайне и переключайте оригинал/копию.</p><button type="button" class="btn ghost sm" disabled={disabled||brushDirty||!projectId||busy||applying||job?.status!=='done'} onclick={apply}>Применить обработанную копию</button>{#if !projectId}<p>Сохраните проект, чтобы применить копию.</p>{/if}{/if}
 {#if token&&ready&&job?.status==='done'&&frameCount}<VideoMaskBrushEditor onnavigation={navigateBrush} ontracked={ontracked?tracked:undefined} autoOpen={brushAutoOpen} {preferredRegionId} initialFrame={preferredFrame} initialView={preferredView} {token} {jobId} {frameRate} width={frameWidth} height={frameHeight} frames={frameCount} disabled={disabled||busy||applying} onchanged={value=>{brushDirty=value}} onready={corrected} />{/if}
 {#if continuing}<p role="status">Подготавливаю новую область…</p><progress aria-label="Подготовка новой области" max="100" value={continuationProgress}></progress><button type="button" class="btn ghost sm" disabled={!continuationId||cancelling} onclick={cancelContinuation}>Отменить подготовку области</button>{/if}
 {#if applying}<p role="status">Сохраняю копию…</p>{/if}
 {#if notice}<p role="status">{notice}</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
 <details><summary>История удаления фона этого видео</summary><button type="button" class="btn ghost sm" disabled={disabled||!token||historyBusy} onclick={()=>loadHistory()}>Обновить историю удаления фона</button><ul aria-label="История удаления фона">{#each history as item (item.jobId)}<li><button type="button" class="btn ghost sm" disabled={disabled||brushDirty||busy||applying} data-job-id={item.jobId} onclick={()=>openHistory(item)}>{labels[item.status]} · {new Date(item.createdAt*1000).toLocaleString('ru-RU')}</button></li>{/each}</ul>{#if historyCursor}<button type="button" class="btn ghost sm" disabled={disabled||historyBusy} onclick={()=>loadHistory(true)}>Ещё удаления фона</button>{/if}{#if historyError}<p role="alert">{historyError}</p>{/if}</details>
 <p>Закрытие панели не отменяет фоновое задание.</p>
</details>

