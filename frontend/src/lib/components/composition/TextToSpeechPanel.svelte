<script lang="ts">
 import { tick,untrack } from 'svelte'
 import TtsReferenceLibrary from './TtsReferenceLibrary.svelte'
 import TtsModelManager from './TtsModelManager.svelte'
 import { observeTtsJob } from '$lib/tts-job-client'
 import { ttsVoiceLabel } from '$lib/tts-voice-label'
 import type { Job } from '$lib/types'
 import { nestedComposition,nestedHistory,insertNestedMedia } from '$lib/state/nestedComposition.svelte'
 import { compositionSourceFromLibraryEntry } from '$lib/state/composition.svelte'
 import type { MediaEntry } from '$lib/types'
 import { getLocalTtsStatus,planVoiceReferenceClip,createLocalTtsReferenceFromSource,uploadLocalTtsReference,startLocalTtsJob,cancelLocalTtsJob,getLocalTtsJobAudio,listLocalTtsJobs,saveLocalNarration,type LocalTtsStatus,type LocalTtsJobSummary } from '$lib/api'
 let {token,disabled=false}:{token:string;disabled?:boolean}=$props()
 let status=$state<LocalTtsStatus|null>(null),text=$state(''),voiceId=$state(3),speed=$state(1),busy=$state(false),loading=$state(false),error=$state(''),audioUrl=$state('')
 let resultBlob:Blob|null=null
 let pauses=$state(0.2)
 let numSteps=$state(5)
 let textInput:HTMLTextAreaElement|undefined
 async function insertExpression(tag:string){
  if(disabled||busy||adding||status?.engine!=='chatterbox-onnx')return
  let at=Math.min(textInput?.selectionStart??text.length,text.length)
  if(at>0&&at<text.length&&/[\uDC00-\uDFFF]/.test(text[at])&&/[\uD800-\uDBFF]/.test(text[at-1]))at--
  const before=text.slice(0,at),after=text.slice(at)
  const inserted=(before&&!/\s$/.test(before)?' ':'')+tag+(/^\s/.test(after)?'':' ')
  text=before+inserted+after
  await tick();textInput?.focus();textInput?.setSelectionRange(at+inserted.length,at+inserted.length)
 }
 let saved: {projectId:string;entry:MediaEntry}|undefined
 let adding=$state(false),notice=$state('')
 let jobId=$state(''),job=$state<Job|null>(null),cancelling=$state(false)
 let history=$state<LocalTtsJobSummary[]>([]),historyCursor=$state<string|null>(null),historyBusy=$state(false),historyError=$state('')
 let historyRequest:AbortController|undefined,cancelRequest:AbortController|undefined
 const active=$derived(job?.status==='pending'||job?.status==='running')
 const labels:Record<Job['status'],string>={pending:'Ожидает',running:'Создаётся речь',done:'Готово',cancelled:'Отменено',error:'Ошибка',interrupted:'Прервано'}
 let referenceFile=$state<File|null>(null),referenceId=$state(''),referenceDuration=$state(0),referenceBusy=$state(false),referenceError=$state('')
 let referenceClipId=$state(''),referenceOrigin=$state<'file'|'clip'>('file')
 const referenceActive=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 const referenceClips=$derived(nestedComposition.document?.compositions[referenceActive]?.content.tracks.flatMap(track=>track.kind==='video'||track.kind==='audio'?track.clips.map(clip=>({id:clip.id,title:nestedComposition.document?.compositions[referenceActive]?.content.metadata?.clipNames?.[clip.id]||track.name||clip.id})):[])??[])
 let previousReferenceDocument:typeof nestedComposition.document|undefined,previousReferenceActive=''
 let referenceRequest:AbortController|undefined
 const referenceMinimum=$derived(voiceId===2099?5000000:500000)
 const needsReference=$derived(voiceId===1099||voiceId===2099)
 let ownedAudioUrl = ''
 let request:AbortController|undefined
 const tooLong=$derived(new TextEncoder().encode(text).length>(status?.maxTextBytes??4096))
 const current=(owner:string,controller:AbortController)=>owner===token&&!controller.signal.aborted
 function clearAudio(){if(ownedAudioUrl)URL.revokeObjectURL(ownedAudioUrl);ownedAudioUrl='';audioUrl=''}
 $effect(()=>{const owner=token;untrack(()=>{request?.abort();cancelRequest?.abort();historyRequest?.abort();referenceRequest?.abort();referenceFile=null;referenceClipId='';referenceOrigin='file';referenceId='';referenceDuration=0;referenceBusy=false;referenceError='';jobId='';job=null;cancelling=false;history=[];historyCursor=null;historyBusy=false;historyError='';clearAudio();status=null;resultBlob=null;saved=undefined;adding=false;notice='';text='';voiceId=3;speed=1;busy=false;loading=false;error=''});return()=>{void owner;request?.abort();cancelRequest?.abort();historyRequest?.abort();referenceRequest?.abort();clearAudio()}})
 $effect(()=>{const document=nestedComposition.document,active=referenceActive;untrack(()=>{if(previousReferenceDocument!==document||previousReferenceActive!==active){referenceClipId='';if(referenceOrigin==='clip'){referenceRequest?.abort();referenceId='';referenceDuration=0;referenceBusy=false;referenceError='Проект или таймлайн изменился. Выберите клип заново.'}previousReferenceDocument=document;previousReferenceActive=active}})})
 async function referenceFromClip(){if(!referenceClipId||referenceBusy||busy||disabled)return;const document=nestedComposition.document,composition=referenceActive,clip=referenceClipId;if(!document||!composition)return;referenceOrigin='clip';referenceRequest?.abort();const controller=new AbortController(),owner=token;referenceRequest=controller;referenceBusy=true;referenceError='';referenceId='';try{const plan=await planVoiceReferenceClip(document,composition,clip,controller.signal);if(!current(owner,controller))return;if(document!==nestedComposition.document||composition!==referenceActive||clip!==referenceClipId)throw new Error('Клип или проект изменился');if(!Number.isSafeInteger(plan.sourceStartUs)||plan.sourceStartUs<0||!Number.isSafeInteger(plan.sourceStartUs+10000000))throw new Error('Некорректное исходное время клипа');const prepared=await createLocalTtsReferenceFromSource(owner,plan.sourceId,plan.sourceStartUs,plan.sourceStartUs+10000000,controller.signal);if(!current(owner,controller))return;if(document!==nestedComposition.document||composition!==referenceActive||clip!==referenceClipId)throw new Error('Клип или проект изменился');if(!prepared.id||!Number.isFinite(prepared.durationUs)||prepared.durationUs<500000||prepared.durationUs>10000000)throw new Error('Сервер не подтвердил образец клипа');referenceId=prepared.id;referenceDuration=prepared.durationUs}catch(reason){if(current(owner,controller))referenceError=reason instanceof Error?reason.message:String(reason)}finally{if(current(owner,controller))referenceBusy=false}}
 async function uploadReference(){if(!referenceFile||referenceBusy||busy||disabled)return;const file=referenceFile;referenceOrigin='file';if(file.size<44||file.size>8*1024*1024){referenceError='Выберите WAV размером до 8 МиБ';return}referenceRequest?.abort();const controller=new AbortController(),owner=token;referenceRequest=controller;referenceBusy=true;referenceError='';referenceId='';try{const uploaded=await uploadLocalTtsReference(owner,file,controller.signal);if(current(owner,controller)){if(!uploaded.id||!Number.isFinite(uploaded.durationUs)||uploaded.durationUs<500000||uploaded.durationUs>10000000)throw new Error('Сервер не подтвердил образец голоса');referenceId=uploaded.id;referenceDuration=uploaded.durationUs}}catch(reason){if(current(owner,controller))referenceError=reason instanceof Error?reason.message:String(reason)}finally{if(current(owner,controller))referenceBusy=false}}
 async function check(){if(busy||loading||disabled)return;const controller=new AbortController(),owner=token;request?.abort();request=controller;loading=true;error='';try{const next=await getLocalTtsStatus(controller.signal,owner);if(current(owner,controller)){status=next;speed=Math.max(next.minSpeed,Math.min(next.maxSpeed,speed));if(!next.voices.some(voice=>voice.id===voiceId))voiceId=next.voices[0]?.id??3}}catch(reason){if(current(owner,controller))error=String(reason)}finally{if(current(owner,controller))loading=false}}
 function accept(blob:Blob){clearAudio();resultBlob=blob;saved=undefined;ownedAudioUrl=URL.createObjectURL(blob);audioUrl=ownedAudioUrl}
 async function watch(id:string,owner:string,controller:AbortController){
  const result=await observeTtsJob(owner,id,controller.signal,update=>{if(current(owner,controller)){job=update;history=history.map(item=>item.jobId===id?{...item,status:update.status,progress:update.progress??null}:item)}})
  if(!current(owner,controller))return
  job=result
  history=history.map(item=>item.jobId===id?{...item,status:result.status,progress:result.progress??null}:item)
  if(result.status==='done'){const blob=await getLocalTtsJobAudio(owner,id,controller.signal);if(current(owner,controller)){accept(blob);notice='Озвучка готова'}}
  else if(result.status==='error'||result.status==='interrupted')throw new Error(result.error||'Не удалось создать озвучку')
 }
 async function generate(){
  if(!token||busy||active||adding||loading||referenceBusy||(needsReference&&(!referenceId||referenceDuration<referenceMinimum))||disabled||tooLong||!text.trim()||!status?.available)return
  const controller=new AbortController(),owner=token;request?.abort();request=controller;busy=true;error='';notice='';jobId='';job=null;clearAudio();resultBlob=null;saved=undefined
  try{const admission=await startLocalTtsJob(owner,{text,voiceId,speed,...(status.supportsPauses&&pauses!==0.2?{pauses}:{}),...(status.engine==='pocket-sherpa-onnx'&&numSteps!==5?{numSteps}:{}),...(needsReference?{referenceId}:{})},controller.signal);if(!current(owner,controller))return;if(!admission.jobId)throw new Error('Сервер не вернул номер озвучки');jobId=admission.jobId;await watch(jobId,owner,controller)}catch(reason){if(current(owner,controller))error=reason instanceof Error?reason.message:String(reason)}finally{if(current(owner,controller))busy=false}
 }
 async function restore(id=jobId){
  if(!id||adding)return
  const owner=token,controller=new AbortController();request?.abort();request=controller;cancelRequest?.abort();cancelling=false;busy=true;loading=false;error='';notice='';jobId=id;job=null;clearAudio();resultBlob=null;saved=undefined
  try{await watch(id,owner,controller)}catch(reason){if(current(owner,controller))error=reason instanceof Error?reason.message:String(reason)}finally{if(current(owner,controller))busy=false}
 }
 async function cancelJob(){
  if(!jobId||cancelling)return
  const id=jobId,owner=token,controller=new AbortController();cancelRequest?.abort();cancelRequest=controller;cancelling=true;error=''
  try{await cancelLocalTtsJob(owner,id,controller.signal);if(current(owner,controller)&&id===jobId){notice='Отмена озвучивания запрошена';if(!busy)void restore(id)}}catch(reason){if(current(owner,controller)&&id===jobId)error=`Не удалось отменить озвучивание: ${String(reason)}`}finally{if(current(owner,controller)&&id===jobId)cancelling=false}
 }
 async function loadHistory(more=false){
  const owner=token,controller=new AbortController();historyRequest?.abort();historyRequest=controller;historyBusy=true;historyError=''
  try{const page=await listLocalTtsJobs(owner,more?historyCursor??undefined:undefined,controller.signal);if(current(owner,controller)){history=more?[...history,...page.jobs.filter(j=>!history.some(old=>old.jobId===j.jobId))]:page.jobs;historyCursor=page.nextCursor}}catch(reason){if(current(owner,controller))historyError=String(reason)}finally{if(current(owner,controller))historyBusy=false}
 }

 function cancel(){request?.abort();busy=false;loading=false;adding=false;notice='Операция отменена'}
 async function addToTimeline(){
  const document=nestedComposition.document,projectId=nestedComposition.projectId,target=nestedComposition.activeCompositionId??document?.root,startUs=nestedComposition.timeUs,blob=resultBlob
  if(!document||!projectId||!target||!blob||busy||adding||disabled||nestedHistory.busy)return
  const controller=new AbortController(),owner=token;request?.abort();request=controller;adding=true;error='';notice=''
  const assertCurrent=()=>{if(!current(owner,controller)||nestedComposition.projectId!==projectId||(nestedComposition.activeCompositionId??nestedComposition.document?.root)!==target)throw new Error('Проект или таймлайн изменился. Озвучка сохранена в медиатеке исходного проекта.')}
  try{
   const entry=saved?.projectId===projectId?saved.entry:await saveLocalNarration(owner,projectId,blob,controller.signal)
   if(!current(owner,controller))return
   saved={projectId,entry};assertCurrent()
   if(nestedComposition.document!==document)throw new Error('Проект изменился во время сохранения. Повторите добавление озвучки.')
   const source=compositionSourceFromLibraryEntry(entry),clipId=`narration-${crypto.randomUUID()}`
   await insertNestedMedia(target,source,{trackId:'',automatic:true,newTrackId:`narration-track-${crypto.randomUUID()}`,clipId,startUs,sourceInUs:0,durationUs:source.durationTicks},assertCurrent)
   assertCurrent()
   if(!nestedComposition.document?.compositions[target]?.content.tracks.some(track=>track.clips.some(clip=>clip.id===clipId)))throw new Error(nestedHistory.error||'Не удалось добавить озвучку в изменившийся проект')
   notice='Озвучка добавлена. Сохраните изменения проекта.'
  }catch(reason){if(current(owner,controller))error=reason instanceof Error?reason.message:String(reason)}finally{if(current(owner,controller))adding=false}
 }
</script>
<details><summary>Озвучить текст локально</summary>
 <p>Создайте речь выбранной локальной моделью, прослушайте её и сохраните WAV. Текст обрабатывается на этом компьютере. Доступные голоса и языки зависят от модели.</p>
 <TtsModelManager {token} disabled={disabled||busy||adding||referenceBusy} onchanged={()=>{status=null;void check()}} />
 <button type="button" class="btn ghost sm" disabled={busy||adding||loading||referenceBusy||disabled} onclick={check}>Проверить голоса озвучивания</button>
 {#if status&&!status.available}<p role="status">Локальная модель озвучивания недоступна.</p>{/if}
 {#if status}<label>Голос<select aria-label="Голос озвучивания" value={voiceId} onchange={event=>{const next=Number(event.currentTarget.value);if(status?.voices.some(voice=>voice.id===next))voiceId=next}} disabled={busy||adding||referenceBusy||disabled}>{#each status.voices as voice (voice.id)}<option value={voice.id}>{ttsVoiceLabel(voice)}</option>{/each}</select></label>{/if}
 {#if needsReference}<TtsReferenceLibrary {token} selectedId={referenceId} disabled={disabled||busy||adding||referenceBusy} onselect={reference=>{referenceOrigin='file';referenceId=reference.id;referenceDuration=reference.durationUs;referenceError=''}} onremoved={id=>{if(referenceId===id){referenceId='';referenceDuration=0}}}/><label>Образец голоса WAV<input type="file" accept=".wav,audio/wav" aria-label="Образец голоса WAV" disabled={busy||adding||referenceBusy||disabled} onchange={event=>{referenceFile=event.currentTarget.files?.[0]??null;referenceOrigin='file';referenceId='';referenceDuration=0;referenceError=''}} /></label>
 <label>Клип для образца голоса<select aria-label="Клип для образца голоса" value={referenceClipId} disabled={busy||adding||referenceBusy||disabled} onchange={event=>{referenceClipId=event.currentTarget.value;referenceOrigin='clip';referenceId='';referenceDuration=0;referenceError=''}}><option value="">Выберите видео или звуковой клип</option>{#each referenceClips as clip (clip.id)}<option value={clip.id}>{clip.title}</option>{/each}</select></label>
 <button type="button" class="btn ghost sm" disabled={!referenceClipId||referenceBusy||busy||adding||disabled} onclick={referenceFromClip}>Взять голос из клипа</button>
 <p>Из клипа берётся до 10 секунд исходного звука от его исходной позиции, без эффектов и изменения скорости.</p>
 <p>Используются первые 10 секунд WAV: минимум {voiceId===2099?'5 секунд':'0,5 секунды'}, 1–2 канала, 8–48 кГц. Образец хранится приватно для фоновых заданий.</p>
 <button type="button" class="btn ghost sm" disabled={!referenceFile||referenceBusy||busy||adding||disabled} onclick={uploadReference}>Загрузить образец голоса</button>
 {#if referenceBusy}<progress aria-label="Загрузка образца голоса"></progress>{/if}
 {#if referenceId}{#if referenceDuration>=referenceMinimum}<p role="status">Образец голоса готов: {(referenceDuration/1000000).toFixed(1)} с</p>{:else}<p role="alert">Для Chatterbox нужен образец от 5 до 10 секунд.</p>{/if}{/if}
 {#if referenceError}<p role="alert">{referenceError}</p>{/if}{/if}
 <label>Текст озвучивания<textarea aria-label="Текст озвучивания" bind:this={textInput} bind:value={text} disabled={busy||adding||disabled} rows="4"></textarea></label>
 {#if status?.engine==='chatterbox-onnx'}<div role="group" aria-label="Выражения голоса">{#each [{tag:'[laugh]',label:'Смех'},{tag:'[chuckle]',label:'Смешок'},{tag:'[sigh]',label:'Вздох'}] as expression}<button type="button" class="btn ghost sm" disabled={busy||adding||disabled} onclick={()=>insertExpression(expression.tag)}>{expression.label}</button>{/each}</div><p>В тексте Chatterbox можно использовать [laugh] — смех, [chuckle] — смешок, [sigh] — вздох. Звучание зависит от голоса и контекста.</p>{/if}
 <label>Скорость речи<input type="number" aria-label="Скорость озвучивания" min={status?.minSpeed??0.5} max={status?.maxSpeed??2} step="0.1" bind:value={speed} disabled={busy||adding||disabled||status?.minSpeed===status?.maxSpeed} /></label>
 {#if status?.supportsPauses}<label>Паузы между предложениями<input aria-label="Паузы между предложениями" type="range" min="0" max="1" step="0.05" bind:value={pauses} disabled={busy||adding||disabled} /></label><p>Паузы: {pauses.toFixed(2)}. 0 — без добавленной паузы, 1 — длинные паузы; обычное значение — 0,2.</p>{/if}
 {#if status?.engine==='pocket-sherpa-onnx'}<label>Качество Pocket<select aria-label="Качество Pocket" value={numSteps} onchange={event=>{const next=Number(event.currentTarget.value);if([3,5,8].includes(next))numSteps=next}} disabled={busy||adding||disabled}><option value={3}>Быстро — 3 шага</option><option value={5}>Обычное — 5 шагов</option><option value={8}>Тщательно — 8 шагов</option></select></label>{/if}
 {#if tooLong}<p role="alert">Текст слишком длинный. Сократите его или создавайте озвучку частями.</p>{/if}
 <button type="button" class="btn ghost sm" disabled={busy||active||adding||loading||referenceBusy||disabled||(needsReference&&(!referenceId||referenceDuration<referenceMinimum))||!status?.available||!text.trim()||tooLong||!Number.isFinite(speed)||speed<0.5||speed>2} onclick={generate}>Создать озвучку</button>
 {#if busy||job}<p role="status">{job?labels[job.status]:'Запускаю озвучивание…'}</p>{/if}
 {#if busy||active}<progress aria-label="Создание озвучки"></progress>{/if}
 {#if jobId&&(active||!job)}<button type="button" class="btn ghost sm" disabled={cancelling} onclick={cancelJob}>Отменить озвучивание</button>{/if}
 {#if jobId&&!busy}<button type="button" class="btn ghost sm" onclick={()=>restore()}>Обновить состояние озвучки</button>{/if}
 <p>Можно закрыть панель: фоновая генерация продолжится. Готовую озвучку можно открыть из истории.</p>
 <details><summary>История озвучивания</summary><button type="button" class="btn ghost sm" disabled={historyBusy} onclick={()=>loadHistory()}>Загрузить историю озвучивания</button>
 {#if historyError}<p role="alert">{historyError}</p>{/if}
 <ul>{#each history as item (item.jobId)}<li><button type="button" disabled={adding} onclick={()=>restore(item.jobId)}>{new Date(item.createdAt*1000).toLocaleString('ru-RU')} — {labels[item.status]}</button></li>{/each}</ul>
 {#if historyCursor}<button type="button" disabled={historyBusy} onclick={()=>loadHistory(true)}>Ещё озвучки</button>{/if}</details>
 {#if audioUrl}<button type="button" class="btn ghost sm" disabled={adding||busy||disabled||nestedHistory.busy||!nestedComposition.projectId||!nestedComposition.document} onclick={addToTimeline}>Добавить озвучку на таймлайн</button>{#if !nestedComposition.projectId}<p>Сохраните проект, чтобы добавить озвучку на таймлайн.</p>{/if}<audio controls src={audioUrl} aria-label="Готовая озвучка"></audio><a href={audioUrl} download="narration.wav">Скачать озвучку WAV</a>{/if}
 {#if adding}<p role="status">Сохраняю и добавляю озвучку…</p><button type="button" class="btn ghost sm" onclick={cancel}>Отменить добавление озвучки</button>{/if}
 {#if notice}<p role="status">{notice}</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>
 label { display:flex; flex-direction:column; gap:0.25rem; margin-block:0.5rem; }
 textarea,select,input,audio { max-width:100%; min-width:0; }
 textarea { width:100%; box-sizing:border-box; }
 p { overflow-wrap:anywhere; }
 audio,progress { display:block; width:100%; margin-block:0.5rem; }
</style>
