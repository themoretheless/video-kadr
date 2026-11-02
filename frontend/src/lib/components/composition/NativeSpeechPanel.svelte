<script lang="ts">
  import SpeechModelManager from './SpeechModelManager.svelte'
  import TextToSpeechPanel from './TextToSpeechPanel.svelte'
  import { getLocalSpeechStatus,transcribeLocalSpeech,observeLocalSpeechJob,cancelLocalSpeechJob,listLocalSpeechJobs,planSpeechClip,type LocalSpeechStatus,type LocalSpeechTranscript,type LocalSpeechJobSummary } from '$lib/api'
  import { state as library } from '$lib/state/store.svelte'
  import { nestedComposition,nestedHistory,addNestedSpeechCaptions,addNestedAlignedSpeechCaptions } from '$lib/state/nestedComposition.svelte'
  let {token,disabled=false}:{token:string;disabled?:boolean}=$props()
  let status=$state<LocalSpeechStatus|null>(null),result=$state<LocalSpeechTranscript|null>(null),sourceId=$state(''),language=$state('auto'),segmentation=$state<'phrase'|'word'>('phrase'),start=$state(0),end=$state(60),placement=$state(0),busy=$state(false),error=$state('')
  let clipId=$state(''),recognizedSourceId=$state(''),jobProgress=$state<number|undefined>(undefined),jobStatus=$state(''),jobId=$state('')
  let savedJobs=$state<LocalSpeechJobSummary[]>([]),nextCursor=$state<string|null>(null),historyBusy=$state(false)
  let historyOwner=''
  $effect(()=>{if(historyOwner!==token){historyOwner=token;controller?.abort();savedJobs=[];nextCursor=null;status=null;result=null;jobId='';pending=undefined;sourceId='';clipId='';busy=false;error=''}})
  const active=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
  const clipChoices=$derived(nestedComposition.document?.compositions[active]?.content.tracks.flatMap(track=>track.kind==='video'||track.kind==='audio'?track.clips.map(clip=>({id:clip.id,title:nestedComposition.document?.compositions[active]?.content.metadata?.clipNames?.[clip.id]||track.name})):[])??[])
  let pending:{target:string;clip:string;source:string;document:typeof nestedComposition.document}|undefined
  let previousActive=''
  $effect(()=>{if(active!==previousActive){if(previousActive){controller?.abort();if(!busy&&jobId&&!result)void cancelLocalSpeechJob(token,jobId).catch(()=>{})}previousActive=active;clipId='';result=null;jobId='';pending=undefined}})
  let alive=true,controller:AbortController|undefined
  $effect(()=>()=>{alive=false;controller?.abort();if(!busy&&jobId&&!result)void cancelLocalSpeechJob(token,jobId).catch(()=>{})})
  async function check(){const owner=token;try{const next=await getLocalSpeechStatus(owner);if(alive&&owner===token)status=next}catch(reason){if(alive&&owner===token)error=String(reason)}}
  async function loadSaved(more=false){
    if(historyBusy||busy)return
    const owner=token,cursor=more?nextCursor:undefined
    if(more&&!cursor)return
    historyBusy=true;error=''
    try{
      const page=await listLocalSpeechJobs(owner,cursor??undefined)
      if(alive&&owner===token){savedJobs=more?[...savedJobs,...page.jobs.filter(job=>!savedJobs.some(existing=>existing.jobId===job.jobId))]:page.jobs;nextCursor=page.nextCursor}
    }catch(reason){if(alive&&owner===token)error=String(reason)}finally{historyBusy=false}
  }
  async function restoreSaved(job:LocalSpeechJobSummary){
    if(busy||disabled||nestedHistory.busy||!nestedComposition.document)return
    clipId='';sourceId=job.sourceId;result=null;jobId=job.jobId;jobProgress=job.progress??undefined;jobStatus=job.status
    pending={target:active,clip:'',source:job.sourceId,document:nestedComposition.document}
    await reconnect()
  }
  function savedLabel(job:LocalSpeechJobSummary){
    const name=library.library.find(entry=>entry.id===job.sourceId)?.title||library.library.find(entry=>entry.id===job.sourceId)?.filename||job.sourceId
    const labels:Record<string,string>={pending:'ожидает',running:'распознаётся',done:'готово',error:'ошибка',cancelled:'отменено',interrupted:'прервано'}
    return `${name} — ${labels[job.status]??job.status}`
  }
  async function recognize(){
    if(busy||disabled||nestedHistory.busy)return
    const owner=token
    busy=true;error='';result=null;jobProgress=undefined;jobStatus='';jobId='';pending=undefined;controller=new AbortController()
    const signal=controller.signal
    try{
      const selectedClip=clipId,target=active
      let startUs=Math.round(start*1000000),endUs=Math.round(end*1000000),source=sourceId
      if(selectedClip){
        const document=nestedComposition.document;if(!document)throw new Error('Нет открытого проекта')
        const plan=await planSpeechClip(document,target,selectedClip)
        if(!alive||owner!==token||signal.aborted)return
        if(nestedComposition.document!==document||active!==target||clipId!==selectedClip)throw new Error('Клип изменился. Повторите распознавание.')
        source=plan.sourceId;startUs=plan.sourceStartUs;endUs=plan.sourceEndUs;sourceId=source;start=startUs/1000000;end=endUs/1000000
      }
      if(!source||!Number.isSafeInteger(startUs)||!Number.isSafeInteger(endUs)||startUs<0||endUs<=startUs||endUs-startUs>600000000)throw new Error('Выберите исходный файл и диапазон до 10 минут')
      pending={target,clip:selectedClip,source,document:nestedComposition.document}
      const transcript=await transcribeLocalSpeech(owner,{sourceId:source,startUs,endUs,language,segmentation},signal,update=>{if(alive&&owner===token&&!signal.aborted){jobProgress=update.progress;jobStatus=update.status;jobId=update.jobId}})
      signal.throwIfAborted()
      if(alive&&owner===token&&active===target&&clipId===selectedClip){recognizedSourceId=source;result=transcript;placement=nestedComposition.timeUs/1000000}
    }catch(reason){if(alive&&owner===token)error=reason instanceof Error?reason.message:String(reason)}finally{if(owner===token)busy=false}
  }
  async function stopDetached(){
    if(!jobId||busy)return
    const owner=token
    busy=true;error=''
    try{await cancelLocalSpeechJob(owner,jobId);if(alive&&owner===token){jobId='';pending=undefined;error='Отмена распознавания запрошена'}}
    catch(reason){if(alive&&owner===token)error=`Не удалось подтвердить отмену: ${String(reason)}`}
    finally{if(owner===token)busy=false}
  }
  async function reconnect(){
    const context=pending,id=jobId
    if(!context||!id||busy||disabled||nestedHistory.busy)return
    if(context.target!==active||context.clip!==clipId||context.source!==sourceId||context.document!==nestedComposition.document){error='Проект или исходник изменился. Повторите распознавание.';return}
    const owner=token
    busy=true;error='';controller=new AbortController()
    const signal=controller.signal
    try{
      const transcript=await observeLocalSpeechJob(owner,id,signal,update=>{if(alive&&owner===token&&!signal.aborted){jobProgress=update.progress;jobStatus=update.status}})
      signal.throwIfAborted()
      if(alive&&owner===token&&pending===context&&active===context.target&&clipId===context.clip&&sourceId===context.source&&nestedComposition.document===context.document){recognizedSourceId=context.source;result=transcript;segmentation=transcript.segmentation??'phrase';start=transcript.sourceStartUs/1000000;end=(transcript.sourceStartUs+transcript.durationUs)/1000000;placement=nestedComposition.timeUs/1000000}
    }catch(reason){if(alive&&owner===token)error=reason instanceof Error?reason.message:String(reason)}finally{if(owner===token)busy=false}
  }
  function editText(index:number,text:string){if(result)result={...result,segments:result.segments.map((segment,i)=>i===index?{...segment,text}:segment)}}
  async function add(){if(!result||!nestedComposition.document)return;if(clipId)await addNestedAlignedSpeechCaptions(active,clipId,recognizedSourceId,result);else await addNestedSpeechCaptions(active,result,Math.round(placement*1000000))}
</script>
<details><summary>Распознать речь локально</summary>
  <p>Whisper работает на этом компьютере. Звук не отправляется в облако. Выберите диапазон исходного файла или клип, по времени которого нужно разместить субтитры.</p>
  <SpeechModelManager {token} {disabled} onchanged={()=>void check()} />
  <button type="button" class="btn ghost sm" disabled={busy} onclick={()=>void check()}>Проверить модель речи</button>
  {#if status}<p role="status">{status.available?`Модель: ${status.model}`:'Модель речи не выбрана.'}</p>{/if}
  <button type="button" class="btn ghost sm" disabled={busy||historyBusy} onclick={()=>void loadSaved()}>Показать сохранённые распознавания</button>
  {#if historyBusy}<p role="status">Загружаю список распознаваний…</p>{/if}
  {#if savedJobs.length}<ul>{#each savedJobs as job (job.jobId)}<li><button type="button" class="btn ghost sm" disabled={busy||disabled||nestedHistory.busy||!['pending','running','done'].includes(job.status)} onclick={()=>void restoreSaved(job)}>{savedLabel(job)}</button></li>{/each}</ul>{/if}
  {#if nextCursor}<button type="button" class="btn ghost sm" disabled={busy||historyBusy} onclick={()=>void loadSaved(true)}>Ещё распознавания</button>{/if}
  <fieldset class="composition-fieldset" disabled={disabled||busy||nestedHistory.busy}>
    <label>Размещение субтитров<select aria-label="Клип для распознанной речи" value={clipId} onchange={event=>{clipId=event.currentTarget.value;result=null}}><option value="">Разместить вручную</option>{#each clipChoices as clip (clip.id)}<option value={clip.id}>{clip.title}</option>{/each}</select></label>
    {#if clipId}<p>Диапазон берётся из выбранного клипа. При добавлении время рассчитывается по текущей скорости, обрезке и направлению воспроизведения клипа.</p>{/if}
    <label>Исходный файл<select disabled={!!clipId} aria-label="Источник распознавания речи" value={sourceId} onchange={event=>{sourceId=event.currentTarget.value;const entry=library.library.find(entry=>entry.id===sourceId);start=0;end=Math.min(entry?.duration??60,600);result=null}}><option value="">Выберите видео или звук</option>{#each library.library.filter(entry=>entry.kind==='source'&&['video','audio'].includes(entry.mediaType??'')) as entry (entry.id)}<option value={entry.id}>{entry.title||entry.filename}</option>{/each}</select></label>
    <label>Разбиение субтитров<select aria-label="Разбиение распознанной речи" value={segmentation} onchange={event=>{const value=event.currentTarget.value;if(value==='phrase'||value==='word')segmentation=value}}><option value="phrase">По фразам</option><option value="word">По словам</option></select></label>
    {#if segmentation==='word'}<p>Каждое слово становится отдельным субтитром. Времена определяет Whisper; для длинной речи выберите короткий диапазон — максимум 512 фрагментов.</p>{/if}
    <label>Язык<select aria-label="Язык распознавания речи" bind:value={language}><option value="auto">Определить автоматически</option><option value="ru">Русский</option><option value="en">Английский</option><option value="hy">Армянский</option></select></label>
    <label>Начало в файле, с<input disabled={!!clipId} aria-label="Начало распознавания речи" type="number" min="0" step="0.001" bind:value={start} /></label>
    <label>Конец в файле, с<input disabled={!!clipId} aria-label="Конец распознавания речи" type="number" min="0" step="0.001" bind:value={end} /></label>
    <button type="button" class="btn ghost sm" disabled={(!sourceId&&!clipId)||!status?.available} onclick={()=>void recognize()}>Распознать речь</button>
    {#if result}
      <p role="status">Язык: {result.language}. Фрагментов: {result.segments.length}.</p>
      {#each result.segments as segment,index (index)}<label>{(segment.startUs/1000000).toFixed(2)}–{(segment.endUs/1000000).toFixed(2)} с<textarea aria-label={`Текст распознанного фрагмента ${index+1}`} value={segment.text} onchange={event=>editText(index,event.currentTarget.value)}></textarea></label>{/each}
      {#if !clipId}<label>Начало субтитров в таймлайне, с<input aria-label="Начало распознанных субтитров" type="number" min="0" step="0.001" bind:value={placement} /></label>{/if}
      <button type="button" class="btn ghost sm" disabled={!result.segments.length} onclick={()=>void add()}>Добавить распознанные субтитры</button>
    {/if}
  </fieldset>
  {#if busy}<p role="status">{jobStatus==='pending'?'Ожидаю начала распознавания…':'Распознаю речь локально…'}</p><progress aria-label="Ход распознавания речи" max="100" value={jobProgress}></progress><button type="button" class="btn ghost sm" onclick={()=>controller?.abort()}>Отменить распознавание</button>{/if}
  {#if error}<p role="alert">{error}</p>{#if jobId&&!busy}<button type="button" class="btn ghost sm" disabled={disabled||nestedHistory.busy} onclick={()=>void reconnect()}>Повторить наблюдение за распознаванием</button><button type="button" class="btn ghost sm" onclick={()=>void stopDetached()}>Остановить распознавание</button>{/if}{/if}
  <TextToSpeechPanel {token} {disabled} />
</details>
