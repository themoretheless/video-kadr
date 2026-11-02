<script lang="ts">
 import {tick} from 'svelte'
 import {withVideoSmartLock} from '$lib/composition/videoSmartLock'
 import {captureSmartVideoBase,smartDocumentHash} from '$lib/composition/smartVideoReceipt'
 import {pendingVideoSmartKey,readPendingVideoSmartRecord,writePendingVideoSmart,removePendingVideoSmart,type PendingVideoSmartStroke,type PendingVideoSmartRecord,type PendingVideoSmartSubmission} from '$lib/composition/pendingVideoSmart'
 import {observeNestedVideoSmartRegion,cancelNestedVideoSmartRegion,cancelNestedVideoSmartOperation} from '$lib/api'
 import {nestedComposition,nestedHistory,reconcileNestedSavedVideoSmartRegion,startNestedSavedVideoSmartRegion,attachNestedSavedVideoSmartRegion,analyseNestedSavedVideoCutout,previewNestedSavedVideoCutout,setNestedSavedVideoCutout} from '$lib/state/nestedComposition.svelte'
 let {token,compositionId,clipId,disabled=false}:{token:string|null;compositionId:string;clipId:string;disabled?:boolean}=$props()
 let busy=$state(false),message=$state(''),canvas=$state<HTMLCanvasElement>(),hasFrame=$state(false)
 const selectedClip=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks.flatMap(track=>track.kind==='video'?track.clips:[]).find(clip=>clip.id===clipId))
 const binding=$derived(selectedClip?.concatCutout)
 let feather=$state(0.01)
 let maskView=$state<'cutout'|'original'|'overlay'>('cutout')
 let editMask=$state(false),custom=$state(false),strokes=$state<import('$lib/api').MaskStroke[]>([])
 let regionTracks=$state<import('$lib/composition/types').ConcatVideoRegionTrack[]>([])
 const trackArgument=$derived(regionTracks.length||binding?.regionTracks?.length?regionTracks:undefined)
 let brushTool=$state<import('$lib/api').MaskStroke['tool']>('brush'),brushSize=$state(0.1)
 let drawing:{pointer:number;tool:import('$lib/api').MaskStroke['tool'];size:number;points:[number,number][]}|null=null
 const correctionOwner=$derived(JSON.stringify([nestedComposition.projectId,compositionId,clipId,binding?.correction,binding?.regionTracks]))
 $effect(()=>{void correctionOwner;custom=binding?.correction?.custom??false;strokes=JSON.parse(JSON.stringify(binding?.correction?.strokes??[]));regionTracks=JSON.parse(JSON.stringify(binding?.regionTracks??[]))})
 const authored=$derived<import('$lib/api').MaskCorrection|undefined>(binding?.correction||custom||strokes.length?{custom,strokes,feather}:undefined)
 const changedStrokes=$derived(JSON.stringify([custom,strokes,regionTracks])!==JSON.stringify([binding?.correction?.custom??false,binding?.correction?.strokes??[],binding?.regionTracks??[]]))
 function point(event:PointerEvent):[number,number]|null{if(!canvas)return null;const rect=canvas.getBoundingClientRect();if(!rect.width||!rect.height)return null;return [Math.max(0,Math.min(1,(event.clientX-rect.left)/rect.width)),Math.max(0,Math.min(1,(event.clientY-rect.top)/rect.height))]}
 function startStroke(event:PointerEvent){if(!editMask||!hasFrame||!token||busy||cancelling||drawing||disabled||nestedHistory.busy||event.button!==0||strokes.length>=1024||strokes.reduce((n,s)=>n+s.points.length,0)>=65536)return;const p=point(event);if(!p)return;event.preventDefault();canvas?.setPointerCapture(event.pointerId);drawing={pointer:event.pointerId,tool:brushTool,size:brushSize,points:[p]}}
 function moveStroke(event:PointerEvent){if(drawing?.pointer!==event.pointerId)return;const p=point(event);if(p&&drawing.points.length<65536-strokes.reduce((n,s)=>n+s.points.length,0))drawing.points.push(p)}
 function endStroke(event:PointerEvent){if(drawing?.pointer!==event.pointerId)return;const {points,tool,size}=drawing;drawing=null;if(canvas?.hasPointerCapture(event.pointerId))canvas.releasePointerCapture(event.pointerId);if(tool==='smartBrush'||tool==='smartEraser'){void smartStroke(points,tool,size)}else{custom=true;strokes=[...strokes,{tool,size,points,at:null}];void run(false)}}

 const validFeather=$derived(Number.isFinite(feather)&&feather>=0&&feather<=0.1)
 let subject=$state<import('$lib/api').ConcatCutoutSubject>('auto')
 let controller:AbortController|undefined
 let smartJob=$state<{id:string|null;owner:string;intent:PendingVideoSmartRecord;key:string|null}|null>(null),cancelling=$state(false)
 let pendingKey=$state<string|null>(null),pending=$state<PendingVideoSmartRecord|null>(null),pendingWarning=$state('')
 let destroyed=false
 $effect(()=>()=>{destroyed=true;controller?.abort();drawing=null})
 $effect(()=>{
  const owner=token,projectId=nestedComposition.projectId,scope={projectId:projectId??'',compositionId,clipId};let active=true
  pendingKey=null;pending=null;pendingWarning=''
  if(owner&&projectId)void pendingVideoSmartKey(owner,scope).then(key=>{if(active&&!destroyed){pendingKey=key;pending=readPendingVideoSmartRecord(localStorage,key,scope)}}).catch(()=>{if(active&&!destroyed)pendingWarning='Восстановление правки недоступно в этом браузере'})
  return()=>{active=false}
 })
 function forgetPending(intent:PendingVideoSmartRecord,key:string|null){
  try{if(key)removePendingVideoSmart(localStorage,key,intent.id);pendingWarning=''}catch{pendingWarning='Не удалось обновить сохранённое ожидание SAM'}
  if(pending?.id===intent.id)pending=null
 }
 function operationOf(intent:PendingVideoSmartRecord){return intent.version===3?intent.id:intent.operationId}
 async function cancelOperation(){
  if(cancelling)return
  const cancelScope=scope,previousError=nestedHistory.error
  const active=smartJob??(pending&&token?{id:pending.version===3?null:pending.started.jobId,owner:token,intent:pending,key:pendingKey}:null);cancelling=true;controller?.abort()
  try{if(active){const operation=operationOf(active.intent);if(operation)await cancelNestedVideoSmartOperation(active.intent.projectId,operation,active.owner);else if(active.id)await cancelNestedVideoSmartRegion(active.owner,active.id);forgetPending(active.intent,active.key);if(!destroyed&&scope===cancelScope){if(nestedHistory.error===previousError)nestedHistory.error='';message='Отмена области SAM запрошена'}}}
  catch(error){if(!destroyed)nestedHistory.error=error instanceof Error?error.message:String(error)}
  finally{if(!destroyed)cancelling=false}
 }
 async function resumeSmart(){
  const intent=pending;if(!intent||!canResume)return
  editMask=true;nestedComposition.timeUs=intent.timeUs;await tick()
  if(!destroyed)void smartStroke(intent.points,intent.tool,intent.size,intent)
 }
 async function smartStroke(points:[number,number][],tool:'smartBrush'|'smartEraser',size:number,resume?:PendingVideoSmartRecord){
  if(!token||disabled||busy||cancelling||destroyed)return
  if(!binding||changedStrokes||feather!==savedFeather||subject!==savedSubject){message='Сначала включите вырезание и примените текущие настройки';return}
  if(pending&&!resume){message='Продолжите или уберите ожидающую правку SAM';return}
  const projectId=nestedComposition.projectId;if(!projectId||nestedComposition.revision===null||!selectedClip)return
  const owner=JSON.stringify([token,projectId,compositionId,clipId]),request=new AbortController(),stamp=scope,timeUs=nestedComposition.timeUs
  controller=request;busy=true;nestedHistory.error='';message=resume?'Продолжение умной видеокисти':'Запуск умной видеокисти';let applied=false
  try{
   const key=await pendingVideoSmartKey(token,{projectId,compositionId,clipId})
   if(request.signal.aborted||stamp!==scope||destroyed)return
   pendingKey=key
   const outcome=await withVideoSmartLock(key,request.signal,async atomic=>{
    if(!atomic)pendingWarning='В этом браузере нет защиты от одновременной правки SAM в разных вкладках'
    let stored:PendingVideoSmartRecord|null=null,raw:string|null=null,readable=false
    try{raw=localStorage.getItem(key);stored=readPendingVideoSmartRecord(localStorage,key,{projectId,compositionId,clipId});readable=true}catch{pendingWarning='Браузер не разрешил проверить сохранённое ожидание SAM'}
    if(readable&&raw!==null&&(!resume||!stored||stored.id!==resume.id)){pending=stored;throw new Error('Есть другая или неизвестная ожидающая правка SAM; повторный запуск остановлен')}
    if(readable&&resume&&raw===null){pending=null;message='Ожидающая правка уже завершена или убрана в другой вкладке';return}
    const record=stored??resume
    let intent:PendingVideoSmartStroke|undefined=record&&record.version!==3?record:undefined
    if(!intent){
     const draft:PendingVideoSmartSubmission=record?.version===3?record:{version:3,id:crypto.randomUUID(),base:await captureSmartVideoBase(nestedComposition.document!,compositionId,clipId),projectId,compositionId,clipId,sourceId:selectedClip!.sourceId,sourceHash:binding!.sourceHash,expectedRevision:nestedComposition.revision!,timeUs,points,tool,size}
     if(request.signal.aborted||stamp!==scope||destroyed)return
     if(record?.version===3&&await smartDocumentHash(nestedComposition.document!)!==draft.base.documentHash)throw new Error('Исходный документ запуска SAM изменился')
     if(request.signal.aborted||stamp!==scope||destroyed)return
     pending=draft;smartJob={id:null,owner:token!,intent:draft,key}
     try{writePendingVideoSmart(localStorage,key,draft)}catch{pendingWarning='Правка продолжится, но браузер не сохранил ожидание'}
     const started=await startNestedSavedVideoSmartRegion(token!,compositionId,clipId,draft.points,request.signal,draft.id)
     if(!started||request.signal.aborted||stamp!==scope||destroyed)return
     if(started.sourceId!==draft.sourceId||started.expectedRevision!==draft.expectedRevision)throw new Error('Сервер вернул другой исходник запуска SAM')
     intent={version:2,operationId:draft.id,base:draft.base,id:draft.id,projectId,compositionId,clipId,sourceHash:draft.sourceHash,timeUs:draft.timeUs,points:draft.points,tool:draft.tool,size:draft.size,started}
     pending=intent
     try{writePendingVideoSmart(localStorage,key,intent)}catch{pendingWarning='Правка продолжится, но браузер не сохранил ожидание'}
    }
    const started=intent.started
    smartJob={id:started.jobId,owner:token!,intent,key}
    const completed=await observeNestedVideoSmartRegion(token!,started.jobId,request.signal,job=>{if(!destroyed&&scope===stamp)message=typeof job.progress==='number'&&Number.isFinite(job.progress)?`Область SAM: ${Math.round(Math.max(0,Math.min(100,job.progress)))}%`:'Обработка области SAM'})
    if(request.signal.aborted||scope!==stamp||destroyed)return
    const reconciled=intent.base?await reconcileNestedSavedVideoSmartRegion(token!,intent,completed,request.signal):'unapplied'
    if(reconciled==='applied')applied=true
    else if(reconciled==='unapplied')applied=await attachNestedSavedVideoSmartRegion(token!,compositionId,clipId,intent.timeUs,started,intent.points,intent.tool,intent.size,request.signal)
    if(applied)forgetPending(intent,key)
   })
   if(outcome==='occupied'&&!destroyed&&scope===stamp){pending=readPendingVideoSmartRecord(localStorage,key,{projectId,compositionId,clipId});message='Правка SAM уже выполняется в другой вкладке'}
  }catch(error){if(!destroyed&&!request.signal.aborted&&scope===stamp)nestedHistory.error=error instanceof Error?error.message:String(error)}
  finally{if(!destroyed&&controller===request){busy=false;smartJob=null;if(request.signal.aborted||nestedHistory.error)message=''}}
  if(applied){await tick();if(!destroyed&&owner===JSON.stringify([token,nestedComposition.projectId,compositionId,clipId]))void run(false)}
 }

 const enabled=$derived(Boolean(binding))
 const savedSubject=$derived(binding?.subject)
 const savedFeather=$derived(binding?.correction?.feather??binding?.feather??(binding?0:0.01))
 const canResume=$derived(Boolean(pending&&binding&&selectedClip?.sourceId===(pending.version===3?pending.sourceId:pending.started.sourceId)&&binding.sourceHash===pending.sourceHash&&(pending.version===3?nestedComposition.revision===pending.expectedRevision:(nestedComposition.revision===pending.started.expectedRevision||(pending.base&&nestedComposition.revision===pending.started.expectedRevision+1)))&&!changedStrokes&&feather===savedFeather&&subject===savedSubject))
 const featherOwner=$derived(JSON.stringify([nestedComposition.projectId,compositionId,clipId,savedFeather]))
 $effect(()=>{void featherOwner;feather=savedFeather})
 let analysed=$state<{owner:string;subject:import('$lib/api').ConcatCutoutSubject;modelHash:string}|null>(null)
 const analysisOwner=$derived(JSON.stringify([token,nestedComposition.projectId,nestedComposition.revision,compositionId,clipId,nestedComposition.document]))
 const ready=$derived(analysed?.owner===analysisOwner&&analysed.subject===subject?analysed:null)
 const canReapply=$derived(Boolean(binding&&(changedStrokes||(ready&&(binding.subject!==ready.subject||binding.modelHash!==ready.modelHash))||(subject===binding.subject&&(binding.feather??0)!==feather))))
 $effect(()=>{void analysisOwner;analysed=null})
 const subjectOwner=$derived(JSON.stringify([nestedComposition.projectId,compositionId,clipId,savedSubject]))
 $effect(()=>{void subjectOwner;subject=savedSubject??'auto'})
 const scope=$derived(JSON.stringify([maskView,editMask,feather,subject,token,nestedComposition.projectId,nestedComposition.revision,compositionId,clipId,nestedComposition.timeUs,nestedComposition.document]))
 $effect(()=>{void scope;message='';hasFrame=false;drawing=null;return()=>{controller?.abort()}})
 async function run(analyse:boolean){
  if(!token||disabled||busy||cancelling||!validFeather)return
  const request=new AbortController(),stamp=scope;controller=request;busy=true;message='';hasFrame=false;if(analyse)analysed=null
  try{
   if(analyse){const result=await analyseNestedSavedVideoCutout(token,compositionId,clipId,request.signal,subject);if(!result||request.signal.aborted||scope!==stamp)return;if(result.analysedFrames===0&&!result.cacheReused&&result.emptyRuns>0){message='В выбранном диапазоне нет кадров';return}if(result.modelHash)analysed={owner:analysisOwner,subject,modelHash:result.modelHash};message=result.analysedFrames?'Анализ готов':'Готовый анализ использован повторно'}
   const settings=JSON.stringify([authored,trackArgument])
   let frame=trackArgument!==undefined?await previewNestedSavedVideoCutout(token,compositionId,clipId,nestedComposition.timeUs,request.signal,feather,authored,editMask?true:undefined,undefined,trackArgument):editMask?await previewNestedSavedVideoCutout(token,compositionId,clipId,nestedComposition.timeUs,request.signal,feather,authored,true):authored?await previewNestedSavedVideoCutout(token,compositionId,clipId,nestedComposition.timeUs,request.signal,feather,authored):await previewNestedSavedVideoCutout(token,compositionId,clipId,nestedComposition.timeUs,request.signal,feather)
   if(editMask&&maskView!=='cutout'&&frame){
    const original=trackArgument!==undefined?await previewNestedSavedVideoCutout(token,compositionId,clipId,nestedComposition.timeUs,request.signal,feather,authored,true,true,trackArgument):await previewNestedSavedVideoCutout(token,compositionId,clipId,nestedComposition.timeUs,request.signal,feather,authored,true,true)
    if(!original||original.width!==frame.width||original.height!==frame.height)throw new Error('Размер исходного кадра изменился')
    if(maskView==='overlay'){
     const rgba=new Uint8ClampedArray(original.rgba)
     for(let i=0;i<rgba.length;i+=4){const tint=0.45*(1-frame.rgba[i+3]!/255);rgba[i]=Math.round(rgba[i]!*(1-tint)+255*tint);rgba[i+1]=Math.round(rgba[i+1]!*(1-tint));rgba[i+2]=Math.round(rgba[i+2]!*(1-tint))}
     frame={...original,rgba}
    }else frame=original
   }
   if(settings!==JSON.stringify([authored,trackArgument]))return
   if(!frame||request.signal.aborted||scope!==stamp||!canvas)return
   canvas.width=frame.width;canvas.height=frame.height
   const context=canvas.getContext('2d');if(!context)throw new Error('Предпросмотр недоступен')
   const image=context.createImageData(frame.width,frame.height);image.data.set(frame.rgba);context.putImageData(image,0,0);hasFrame=true
  }catch(error){if(!request.signal.aborted&&scope===stamp)nestedHistory.error=error instanceof Error?error.message:String(error)}
  finally{if(controller===request)busy=false}
 }
 async function apply(wanted:boolean){
  if(!token||disabled||busy||(wanted&&!validFeather))return
  const request=new AbortController();controller=request;busy=true
  try{if(wanted&&trackArgument!==undefined)await setNestedSavedVideoCutout(token,compositionId,clipId,wanted,request.signal,subject,ready?.modelHash??binding?.modelHash,feather,authored,trackArgument);else if(wanted&&authored)await setNestedSavedVideoCutout(token,compositionId,clipId,wanted,request.signal,subject,ready?.modelHash??binding?.modelHash,feather,authored);else await setNestedSavedVideoCutout(token,compositionId,clipId,wanted,request.signal,subject,wanted?(ready?.modelHash??binding?.modelHash):undefined,wanted?feather:undefined)}
  finally{if(controller===request)busy=false}
 }
</script>
<fieldset disabled={disabled||busy||cancelling||nestedHistory.busy||!token||!nestedComposition.projectId||nestedComposition.revision===null}>
 <legend>Удаление фона видео</legend>
 {#if pending&&!busy}
  <p>Есть незавершённая правка SAM.</p>
  <button class="btn ghost sm" type="button" disabled={!canResume} onclick={()=>void resumeSmart()}>Продолжить правку SAM</button>
  <button class="btn ghost sm" type="button" onclick={()=>void cancelOperation()}>Отменить правку SAM</button>
  <button class="btn ghost sm" type="button" onclick={()=>{if(pending)forgetPending(pending,pendingKey)}}>Убрать из ожидания</button>
  {#if !canResume}<p>Для продолжения нужна исходная ревизия или подтверждённая сохранённая правка без локальных изменений.</p>{/if}
 {/if}
 {#if pendingWarning}<p role="status">{pendingWarning}</p>{/if}
 <label>Объект анализа<select aria-label="Объект удаления фона видео" value={subject} onchange={event=>{subject=event.currentTarget.value as import('$lib/api').ConcatCutoutSubject}}><option value="auto">Автоматически</option><option value="person">Человек</option><option value="object">Предмет</option></select></label>
 <label>Растушёвка края<input aria-label="Растушёвка края видео" type="number" min="0" max="0.1" step="0.001" bind:value={feather}/></label>
 <label><input type="checkbox" bind:checked={editMask}/>Править маску на исходном кадре</label>
 {#if editMask}
  <label>Вид кадра<select aria-label="Вид видеомаски" value={maskView} onchange={event=>{maskView=event.currentTarget.value as 'cutout'|'original'|'overlay'}}><option value="cutout">Вырезанный кадр</option><option value="original">Исходный кадр</option><option value="overlay">Маска на исходнике</option></select></label>
  <p>В режиме маски удаляемые области отмечены красным. После смены вида нажмите «Показать готовый кадр на курсоре».</p>
  <label>Инструмент<select aria-label="Кисть видеомаски" value={brushTool} onchange={event=>{brushTool=event.currentTarget.value as import('$lib/api').MaskStroke['tool']}}><option value="brush">Добавить область</option><option value="eraser">Стереть область</option><option value="smartBrush" disabled={!binding||changedStrokes||feather!==savedFeather||subject!==savedSubject}>Выделить объект SAM</option><option value="smartEraser" disabled={!binding||changedStrokes||feather!==savedFeather||subject!==savedSubject}>Удалить объект SAM</option></select></label>
  <label>Размер кисти<input aria-label="Размер кисти видеомаски" type="range" min="0.005" max="0.5" step="0.005" bind:value={brushSize}/></label>
  <button type="button" class="btn ghost sm" disabled={!strokes.length} onclick={()=>{strokes=strokes.slice(0,-1);regionTracks=regionTracks.filter(t=>t.stroke<strokes.length);void run(false)}}>Убрать последний штрих</button>
  <button type="button" class="btn ghost sm" disabled={!custom&&!strokes.length} onclick={()=>{strokes=[];regionTracks=[];custom=false;void run(false)}}>Вернуть автоматическую маску</button>
  <p>Покажите кадр и рисуйте по нему. Штрихи действуют на весь клип. Обычную кисть сохраните кнопкой применения настроек; готовая область SAM сохраняется автоматически.</p>
 {/if}
 <button class="btn ghost sm" type="button" disabled={!validFeather} onclick={()=>void run(true)}>Анализировать и показать клип</button>
 <button class="btn ghost sm" type="button" disabled={!validFeather} onclick={()=>void run(false)}>Показать готовый кадр на курсоре</button>
 <button class="btn ghost sm" type="button" aria-pressed={enabled} disabled={!enabled&&!validFeather} onclick={()=>void apply(!enabled)}>{enabled?'Выключить вырезание в проекте':'Включить вырезание в проекте'}</button>
 {#if canReapply}<button class="btn ghost sm" type="button" disabled={!validFeather} onclick={()=>void apply(true)}>Применить настройки вырезания</button>{/if}
</fieldset>
{#if busy||cancelling}<button class="btn ghost sm" type="button" disabled={cancelling} onclick={()=>void cancelOperation()}>Отменить обработку видео</button>{/if}
{#if message}<p role="status">{message}</p>{/if}
<canvas bind:this={canvas} hidden={!hasFrame} aria-label={editMask?"Правка видеомаски на исходном кадре":"Выбранный видеоклип без фона"} onpointerdown={startStroke} onpointermove={moveStroke} onpointerup={endStroke} onpointercancel={()=>{drawing=null}}></canvas>
<style>canvas{touch-action:none;max-width:100%;height:auto;background:repeating-conic-gradient(#777 0% 25%,#aaa 0% 50%) 50%/16px 16px}</style>
