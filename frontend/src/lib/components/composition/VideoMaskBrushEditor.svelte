<script lang="ts">
 import {isVideoMaskNavigation,isVideoMaskFrameRate,videoMaskFrameAtTime,videoMaskFrameRange,videoMaskTime,type VideoMaskFrameRate,type VideoMaskNavigation} from '$lib/video-mask-clock'
 import SamVideoTrackingTool from './SamVideoTrackingTool.svelte'
 import {untrack} from 'svelte'
 import type {SamVideoRegionAttachment} from '$lib/composition/types'
 import {getVideoRegionFrame,getVideoSourceFrame,getVideoCorrectionRecipe,getVideoMaskFrame,startVideoCorrection,type VideoFrameCorrection,type MaskCorrection,type MaskStroke} from '$lib/api'
 import {canonicalVideoRecipe,validateVideoRecipe,copyVideoCorrectionRange,copyVideoRegionCorrectionRange} from '$lib/video-mask-recipe'
 let{token,jobId,width,height,frames,frameRate,disabled=false,autoOpen=false,preferredRegionId='',initialFrame=0,initialView='mask',onready,onchanged,ontracked,onnavigation}:{token:string;jobId:string;width:number;height:number;frames:number;frameRate?:VideoMaskFrameRate;disabled?:boolean;autoOpen?:boolean;preferredRegionId?:string;initialFrame?:number;initialView?:VideoMaskNavigation['view'];onready:(id:string,navigation?:VideoMaskNavigation)=>void;onchanged:(dirty:boolean)=>void;onnavigation?:(id:string,navigation:VideoMaskNavigation,regionId:string)=>void;ontracked?:(entry:import('$lib/api').SavedSamVideoRegion,expected:import('$lib/composition/types').VideoMaskAttachment,samJobId?:string,corrections?:VideoFrameCorrection[],navigation?:VideoMaskNavigation)=>Promise<void>}=$props()
 let rangeBusy=$state(false),rangeDone=$state(0),rangeTotal=$state(0)
 let rangeRequest:AbortController|undefined
 let rangeStart=$state('0'),rangeEnd=$state(''),view=$state<'mask'|'source'|'overlay'>('mask'),sourceImage=$state('')
 let loaded=$state(false),loading=$state(false),saving=$state(false),frameBusy=$state(false),frame=$state(0),error=$state(''),image=$state(''),base=$state(''),maskHash=$state(''),baseline=$state('[]')
 let projectOrigin=$state<import('$lib/api').VideoCorrectionRecipe['projectOrigin']>(undefined),trackingBusy=$state(false),pickTrackingPoint=$state(false),trackingPoint=$state<[number,number]|null>(null)
 let trackedRegions=$state<readonly SamVideoRegionAttachment[]>([])
 let selectedRegion=$state(''),regionImage=$state(''),regionHash=$state(''),regionLoadedSlot=$state(''),regionLoading=$state(false)
 let regionRequest:AbortController|undefined,ownedRegionUrl=''

 let edits=$state<VideoFrameCorrection[]>([]),past=$state<VideoFrameCorrection[][]>([]),future=$state<VideoFrameCorrection[][]>([]),tool=$state<MaskStroke['tool']>('brush'),size=$state(.08),draft=$state<MaskStroke|null>(null),cursor=$state<[number,number]>([.5,.5])
 let expanded=$state(false)
 let autoOpenedScope=''
 let request:AbortController|undefined,preview:AbortController|undefined,pointer:number|undefined,ownedUrl='',ownedSourceUrl=''
 const scope=$derived(JSON.stringify([token,jobId,width,height,frames,frameRate]))
 const regionSlot=$derived(JSON.stringify([scope,frame,selectedRegion]))
 const currentRegion=$derived(trackedRegions.find(r=>r.maskSourceId===selectedRegion))
 const regionReady=$derived(Boolean(regionImage)&&regionLoadedSlot===regionSlot)
 const recipe=$derived<MaskCorrection>(edits.find(e=>e.frame===frame)?.correction??{custom:false,strokes:[],feather:0})
 const dirty=$derived(loaded&&(Boolean(draft)||canonicalVideoRecipe(edits)!==baseline))
 const locked=$derived(rangeBusy||trackingBusy||disabled||loading||saving||frameBusy||!loaded||!image||(view!=='mask'&&!sourceImage))
 const count=$derived(edits.reduce((n,e)=>n+e.correction.strokes.reduce((m,s)=>m+s.points.length,0),0))
 const canvasWidth=$derived(Math.min(480,360*width/height))
 const clone=(v:VideoFrameCorrection[])=>JSON.parse(JSON.stringify(v)) as VideoFrameCorrection[]
 function message(reason:unknown){return reason instanceof Error?reason.message:String(reason)}
 function clearRegion(){regionRequest?.abort();if(ownedRegionUrl)URL.revokeObjectURL(ownedRegionUrl);ownedRegionUrl='';regionImage='';regionHash='';regionLoadedSlot='';regionLoading=false}
 async function showRegion(){if(locked||draft||regionLoading||!currentRegion)return;const captured=scope,ordinal=frame,id=selectedRegion,slot=regionSlot;clearRegion();const controller=new AbortController();regionRequest=controller;regionLoading=true;error='';try{const value=await getVideoRegionFrame(token,jobId,id,ordinal,controller.signal);if(!current(captured,controller)||ordinal!==frame||id!==selectedRegion)return;ownedRegionUrl=URL.createObjectURL(value.png);regionImage=ownedRegionUrl;regionHash=value.maskHash;regionLoadedSlot=slot}catch(reason){if(current(captured,controller)&&ordinal===frame&&id===selectedRegion)error=message(reason)}finally{if(current(captured,controller)&&ordinal===frame&&id===selectedRegion)regionLoading=false}}
 function applyRegion(tool:'smartBrush'|'smartEraser'){if(locked||draft||regionLoading||!regionReady||!currentRegion)return;const r=currentRegion,points=r.points.map(p=>[p[0],p[1]] as [number,number]);commit({...recipe,custom:true,strokes:[...recipe.strokes,{tool,size,points,at:null,region:{sourceId:r.sourceId,maskSourceId:r.maskSourceId,sourceHash:r.sourceHash,maskHash:regionHash,maskFileHash:r.maskFileHash,encoderHash:r.encoderHash,decoderHash:r.decoderHash,runtimeHash:r.runtimeHash,promptHash:r.promptHash,points}}]})}
 function clearImage(){if(ownedUrl)URL.revokeObjectURL(ownedUrl);if(ownedSourceUrl)URL.revokeObjectURL(ownedSourceUrl);ownedUrl='';ownedSourceUrl='';image='';sourceImage=''}
 function current(captured:string,controller:AbortController){return !controller.signal.aborted&&scope===captured}
 $effect(()=>{const captured=scope;untrack(()=>{request?.abort();preview?.abort();rangeRequest?.abort();clearImage();clearRegion();selectedRegion='';loaded=false;loading=false;saving=false;frameBusy=false;frame=isVideoMaskNavigation({frame:initialFrame,view:initialView},frames)?initialFrame:0;view=isVideoMaskNavigation({frame:initialFrame,view:initialView},frames)?initialView:'mask';rangeStart='0';rangeEnd='';rangeBusy=false;rangeDone=0;rangeTotal=0;projectOrigin=undefined;trackingBusy=false;pickTrackingPoint=false;trackingPoint=null;trackedRegions=[];edits=[];past=[];future=[];base='';maskHash='';baseline='[]';draft=null;pointer=undefined;error=''});return()=>{void captured;request?.abort();preview?.abort();rangeRequest?.abort();clearImage();clearRegion();untrack(()=>onchanged(false))}})
 $effect(()=>{const slot=regionSlot;untrack(()=>{void slot;clearRegion()})})
 $effect(()=>{const changed=dirty||trackingBusy||rangeBusy;untrack(()=>onchanged(changed))})
 $effect(()=>{const opened=loaded&&!loading,id=jobId,navigation={frame,view},region=selectedRegion;if(opened)untrack(()=>onnavigation?.(id,navigation,region))})
 $effect(()=>{const ordinal=frame,root=base,opened=loaded,captured=scope,mode=view;if(opened)untrack(()=>void showFrame(ordinal,root,captured,mode))})
 async function showFrame(ordinal:number,root:string,captured:string,mode=view){const controller=new AbortController();preview?.abort();preview=controller;clearImage();frameBusy=true;error='';try{const[blob,source]=await Promise.all([getVideoMaskFrame(token,root,ordinal,controller.signal),mode==='mask'?Promise.resolve(null):getVideoSourceFrame(token,jobId,ordinal,controller.signal)]);if(!current(captured,controller)||frame!==ordinal||view!==mode)return;ownedUrl=URL.createObjectURL(blob);image=ownedUrl;if(source){ownedSourceUrl=URL.createObjectURL(source);sourceImage=ownedSourceUrl}}catch(reason){if(current(captured,controller)&&frame===ordinal&&view===mode)error=message(reason)}finally{if(current(captured,controller)&&frame===ordinal&&view===mode)frameBusy=false}}
 async function load(){if(disabled||loading||saving||dirty)return;const captured=scope,controller=new AbortController();request?.abort();request=controller;preview?.abort();clearImage();loaded=false;frameBusy=false;loading=true;error='';try{const value=await getVideoCorrectionRecipe(token,jobId,controller.signal);if(!current(captured,controller))return;validateVideoRecipe(value,frames);projectOrigin=value.projectOrigin;trackedRegions=[...(value.projectOrigin?.snapshot.trackedRegions??[])];selectedRegion=trackedRegions.some(r=>r.maskSourceId===preferredRegionId)?preferredRegionId:trackedRegions[0]?.maskSourceId??'';edits=clone(value.corrections);baseline=canonicalVideoRecipe(edits);base=value.baseJobId;maskHash=value.maskHash;loaded=true;past=[];future=[]}catch(reason){if(current(captured,controller))error=message(reason)}finally{if(current(captured,controller))loading=false}}
 $effect(()=>{const captured=scope,open=autoOpen,blocked=disabled;untrack(()=>{if(open&&!blocked&&captured===scope&&autoOpenedScope!==captured){autoOpenedScope=captured;expanded=true;void load()}})})
 $effect(()=>{const ordinal=frame;untrack(()=>{void ordinal;trackingPoint=null;pickTrackingPoint=false})})
 async function attachTracked(entry:import('$lib/api').SavedSamVideoRegion,samJobId?:string){if(!ontracked||!projectOrigin||dirty)throw new Error('Сначала пересчитайте или отмените ручные правки');await ontracked(entry,projectOrigin.snapshot,samJobId,clone(edits),{frame,view})}
 function commit(correction:MaskCorrection){if(locked)return;const next=edits.filter(e=>e.frame!==frame);if(correction.custom||correction.strokes.length||correction.feather)next.push({frame,correction});try{validateVideoRecipe({baseJobId:base,maskHash,corrections:next},frames,trackedRegions);past=[...past,clone(edits)];future=[];edits=next}catch(reason){error=message(reason)}}
 function chooseTime(event:Event){if(!frameRate)return;const input=event.currentTarget as HTMLInputElement;try{frame=videoMaskFrameAtTime(input.value,frameRate,frames);input.value=videoMaskTime(frame,frameRate);error=''}catch(reason){input.value=videoMaskTime(frame,frameRate);error=message(reason)}}
 $effect(()=>{if(disabled)rangeRequest?.abort()})
 async function applyRange(){
  if(locked||draft||!frameRate)return
  const captured=scope,controller=new AbortController(),snapshot=clone(edits),identity=canonicalVideoRecipe(snapshot),root=base,hash=maskHash,regions=JSON.parse(JSON.stringify(trackedRegions)) as SamVideoRegionAttachment[],ordinal=frame,owner=token,job=jobId
  rangeRequest?.abort();rangeRequest=controller;error=''
  try{
   const[start,end]=videoMaskFrameRange(rangeStart,rangeEnd,frameRate,frames)
   let next:VideoFrameCorrection[]
   if(snapshot.find(e=>e.frame===ordinal)?.correction.strokes.some(s=>s.region)){
    rangeBusy=true;rangeDone=0;rangeTotal=0
    next=await copyVideoRegionCorrectionRange({baseJobId:root,maskHash:hash,corrections:snapshot},frames,ordinal,start,end,regions,async(id,index,signal)=>(await getVideoRegionFrame(owner,job,id,index,signal)).maskHash,controller.signal,(done,total)=>{if(current(captured,controller)){rangeDone=done;rangeTotal=total}})
   }else next=copyVideoCorrectionRange({baseJobId:root,maskHash:hash,corrections:snapshot},frames,ordinal,start,end,regions)
   if(!current(captured,controller)||disabled||canonicalVideoRecipe(edits)!==identity)return
   if(canonicalVideoRecipe(next)!==identity){past=[...past,snapshot];future=[];edits=next}
  }catch(reason){if(current(captured,controller))error=message(reason)}finally{if(scope===captured&&rangeRequest===controller)rangeBusy=false}
 }
 function cancelRange(){rangeRequest?.abort();error='Применение диапазона отменено'}

 function point(event:PointerEvent):[number,number]{const box=(event.currentTarget as HTMLElement).getBoundingClientRect();if(box.width<=0||box.height<=0)throw new Error('Полотно ещё не открыто');return [Math.max(-.5,Math.min(1.5,(event.clientX-box.left)/box.width)),Math.max(-.5,Math.min(1.5,(event.clientY-box.top)/box.height))]}
 function begin(event:PointerEvent){if(locked||event.button!==0||pointer!==undefined)return;try{if(pickTrackingPoint){trackingPoint=point(event);cursor=trackingPoint;pickTrackingPoint=false;event.preventDefault();return;}draft={tool,size,points:[point(event)],at:null};pointer=event.pointerId;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);event.preventDefault()}catch(reason){error=message(reason);draft=null;pointer=undefined}}
 function move(event:PointerEvent){if(locked||pointer!==event.pointerId||!draft)return;const p=point(event),last=draft.points.at(-1)!;cursor=p;if(Math.hypot(p[0]-last[0],p[1]-last[1])<.002)return;if(count+draft.points.length>=65536){error='Достигнут предел точек';return}draft={...draft,points:[...draft.points,p]}}
 function finish(event:PointerEvent){if(pointer!==event.pointerId)return;if(!locked&&draft){move(event);commit({...recipe,custom:true,strokes:[...recipe.strokes,draft]})}draft=null;pointer=undefined;const canvas=event.currentTarget as HTMLElement;if(canvas.hasPointerCapture?.(event.pointerId))canvas.releasePointerCapture(event.pointerId)}
 function cancelPointer(){draft=null;pointer=undefined}
 function keyboard(event:KeyboardEvent){if(locked||draft)return;const step=event.shiftKey ? 0.1 : 0.01,directions:Record<string,[number,number]>={ArrowLeft:[-step,0],ArrowRight:[step,0],ArrowUp:[0,-step],ArrowDown:[0,step]};const d=directions[event.key];if(d){event.preventDefault();cursor=[Math.max(0,Math.min(1,cursor[0]+d[0])),Math.max(0,Math.min(1,cursor[1]+d[1]))]}else if((event.key==='Enter'||event.key===' ')&&!event.repeat){event.preventDefault();if(pickTrackingPoint){trackingPoint=[...cursor];pickTrackingPoint=false;return;}commit({...recipe,custom:true,strokes:[...recipe.strokes,{tool,size,points:[[...cursor]],at:null}]})}}
 function undo(){if(locked||draft||!past.length)return;future=[...future,clone(edits)];edits=past.at(-1)!;past=past.slice(0,-1)}
 function redo(){if(locked||draft||!future.length)return;past=[...past,clone(edits)];edits=future.at(-1)!;future=future.slice(0,-1)}
 function path(stroke:MaskStroke){return stroke.points.map(([x,y],i)=>`${i?'L':'M'}${x*width} ${y*height}`).join(' ')+(stroke.points.length===1?` L${stroke.points[0]![0]*width} ${stroke.points[0]![1]*height}`:'')}
 async function compute(){if(locked||draft||!dirty)return;const captured=scope,controller=new AbortController(),snapshot=clone(edits),navigation={frame,view};request?.abort();request=controller;saving=true;error='';try{validateVideoRecipe({baseJobId:base,maskHash,corrections:snapshot},frames,trackedRegions);const result=await startVideoCorrection(token,jobId,snapshot,controller.signal);if(!current(captured,controller))return;if(!result.jobId)throw new Error('Сервер не вернул ID правки');onready(result.jobId,navigation)}catch(reason){if(current(captured,controller))error=message(reason)}finally{if(current(captured,controller))saving=false}}
</script>
<details bind:open={expanded}><summary>Правка видеомаски</summary>
 <p>Выберите кадр и рисуйте по его исходной автоматической маске. Белое сохраняется, чёрное удаляется. Цветные линии показывают штрихи; пересчёт создаёт отдельную видеокопию.</p>
 <button type="button" class="btn ghost sm" disabled={disabled||loading||saving||rangeBusy||trackingBusy||dirty} onclick={load}>Открыть видеомаски</button>
 {#if loading||frameBusy}<p role="status">Открываю маску кадра…</p>{/if}
 {#if loaded}
 {#if (!image||(view!=='mask'&&!sourceImage))&&error}<button type="button" class="btn ghost sm" disabled={disabled||saving||frameBusy} onclick={()=>void showFrame(frame,base,scope)}>Повторить загрузку кадра</button>{/if}
 <label>Кадр (с нуля)<input aria-label="Кадр видеомаски" type="number" min="0" max={frames-1} step="1" value={frame} disabled={disabled||saving||trackingBusy||rangeBusy||!!draft} onchange={event=>{const n=Number(event.currentTarget.value);if(Number.isSafeInteger(n)&&n>=0&&n<frames)frame=n;else event.currentTarget.value=String(frame)}}/></label>
 {#if frameRate&&isVideoMaskFrameRate(frameRate)}
 <label>Время от начала исходника, с<input aria-label="Время кадра видеомаски" type="text" inputmode="decimal" value={videoMaskTime(frame,frameRate)} disabled={disabled||saving||trackingBusy||rangeBusy||!!draft} onchange={chooseTime}/></label>
 <div class="actions"><button type="button" class="btn ghost sm" disabled={disabled||saving||trackingBusy||rangeBusy||!!draft||frame===0} onclick={()=>frame--}>Предыдущий кадр маски</button><button type="button" class="btn ghost sm" disabled={disabled||saving||trackingBusy||rangeBusy||!!draft||frame===frames-1} onclick={()=>frame++}>Следующий кадр маски</button></div>
 <details><summary>Правка диапазона видео</summary><p>Правка выбранного кадра заменит правки кадров в диапазоне. Конец диапазона не включается. Кадр без правок возвращает автоматическую маску.</p>
 <label>От времени, с<input aria-label="Начало диапазона видеомаски" type="text" inputmode="decimal" bind:value={rangeStart} disabled={locked||!!draft}/></label>
 <label>До времени, с<input aria-label="Конец диапазона видеомаски" placeholder="До конца видео" type="text" inputmode="decimal" bind:value={rangeEnd} disabled={locked||!!draft}/></label>
 <button type="button" class="btn ghost sm" disabled={locked||!!draft} onclick={applyRange}>Заменить правки диапазона</button>
 {#if rangeBusy}<progress aria-label="Привязка областей диапазона" max={Math.max(1,rangeTotal)} value={rangeDone}></progress><p role="status">Подготовлено масок: {rangeDone} из {rangeTotal}.</p><button type="button" class="btn ghost sm" onclick={cancelRange}>Отменить применение диапазона</button>{/if}</details>
 {/if}
 <label>Основа полотна<select aria-label="Основа полотна видеомаски" value={view} disabled={disabled||loading||saving||trackingBusy||rangeBusy||!!draft} onchange={event=>{const value=event.currentTarget.value;if(value==='mask'||value==='source'||value==='overlay')view=value}}><option value="mask">Автоматическая маска</option><option value="source">Исходный кадр</option><option value="overlay">Исходник и маска</option></select></label>
 {#if ontracked&&projectOrigin&&frameRate}
 <details><summary>Новая отслеживаемая область</summary>
 <p>Выберите точку объекта на исходном кадре. После трекинга новая область откроется для правки автоматически.</p>
 {#if dirty}<p>Сначала пересчитайте или отмените ручные правки.</p>{/if}
 <button type="button" class="btn ghost sm" disabled={locked||dirty||!!draft||view==='mask'} onclick={()=>pickTrackingPoint=!pickTrackingPoint}>{pickTrackingPoint?'Отменить выбор точки SAM':'Выбрать точку объекта SAM'}</button>
 {#if pickTrackingPoint}<p role="status">Нажмите на объект в исходном кадре.</p>{/if}
 {#if trackingPoint}<p>Точка объекта выбрана на кадре {frame}.</p>{/if}
 <SamVideoTrackingTool {token} projectId={projectOrigin.projectId} sourceId={projectOrigin.snapshot.sourceId} sourceHash={projectOrigin.snapshot.sourceHash} targetKey={scope} seedFrame={frame} {width} {height} {frames} {frameRate} points={trackingPoint?[trackingPoint]:undefined} disabled={disabled||loading||saving||rangeBusy||frameBusy||dirty||!!draft} onready={attachTracked} onbusy={value=>trackingBusy=value}/>
 </details>
 {/if}
 {#if trackedRegions.length}
 <fieldset disabled={disabled||saving||loading||!!draft}><legend>Сохранённая область</legend>
 <label>Область<select aria-label="Сохранённая область видеомаски" value={selectedRegion} onchange={event=>selectedRegion=event.currentTarget.value}>{#each trackedRegions as region,i (region.maskSourceId)}<option value={region.maskSourceId}>Область {i+1}</option>{/each}</select></label>
 {#if currentRegion}<p>Отслежено {currentRegion.trackedFrames} из {currentRegion.frames} кадров.</p>{/if}
 <button type="button" class="btn ghost sm" disabled={locked||regionLoading} onclick={showRegion}>Показать область кадра</button>
 {#if regionLoading}<p role="status">Открываю область кадра…</p>{/if}
 {#if regionReady}<button type="button" class="btn ghost sm" onclick={clearRegion}>Скрыть область</button>{/if}
 <div class="actions"><button type="button" class="btn ghost sm" disabled={locked||regionLoading||!regionReady} onclick={()=>applyRegion('smartBrush')}>Сохранить область кадра</button><button type="button" class="btn ghost sm" disabled={locked||regionLoading||!regionReady} onclick={()=>applyRegion('smartEraser')}>Удалить область кадра</button></div>
 </fieldset>
 {/if}
 <fieldset disabled={locked}>
 <label>Инструмент<select aria-label="Инструмент видеомаски" value={tool} onchange={event=>{const t=event.currentTarget.value;if(t==='brush'||t==='eraser'||t==='smartBrush'||t==='smartEraser')tool=t}}><option value="brush">Сохранить</option><option value="eraser">Удалить</option><option value="smartBrush">Сохранить вероятный объект</option><option value="smartEraser">Убрать сомнительное</option></select></label>
 <label>Диаметр кисти<input aria-label="Диаметр кисти видеомаски" type="range" min="0.005" max="0.5" step="0.005" bind:value={size}/></label>
 <label>Растушёвка кадра<input aria-label="Растушёвка видеомаски" type="range" min="0" max="0.1" step="0.001" value={recipe.feather} onchange={event=>commit({...recipe,feather:Number(event.currentTarget.value)})}/></label>
 {#if image&&(view==='mask'||sourceImage)}<button type="button" class="canvas" aria-label="Полотно видеомаски" style:width={`min(100%, ${canvasWidth}px)`} onpointerdown={begin} onpointermove={move} onpointerup={finish} onpointercancel={cancelPointer} onlostpointercapture={cancelPointer} onkeydown={keyboard}><svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none"><title>Штрихи выбранного кадра видеомаски</title>{#if view!=='mask'}<image href={sourceImage} x="0" y="0" {width} {height} preserveAspectRatio="none"/>{/if}{#if view!=='source'}<image href={image} x="0" y="0" {width} {height} preserveAspectRatio="none" opacity={view==='overlay'?.45:1}/>{/if}{#if regionReady}<image data-video-region-preview href={regionImage} x="0" y="0" {width} {height} preserveAspectRatio="none" opacity="0.45"/>{/if}{#each (recipe.custom?recipe.strokes:[]) as stroke,i (i)}<path d={path(stroke)} stroke={stroke.tool==='eraser'||stroke.tool==='smartEraser'?'var(--danger)':'var(--ok)'} stroke-width={stroke.size*width} stroke-linecap="round" stroke-linejoin="round" fill="none" opacity="0.65"/>{/each}{#if draft}<path d={path(draft)} stroke="var(--accent)" stroke-width={draft.size*width} stroke-linecap="round" stroke-linejoin="round" fill="none"/>{/if}<circle cx={cursor[0]*width} cy={cursor[1]*height} r={size*width/2} fill="none" stroke="var(--text)" stroke-width={width/480}/></svg></button>{/if}
 <p>Стрелки перемещают кисть, Enter ставит точку. Изменено кадров: {edits.length}.{#if dirty}{' '}Правки ещё не пересчитаны.{/if}</p>
 <div class="actions"><button type="button" class="btn ghost sm" disabled={locked||!!draft||!past.length} onclick={undo}>Отменить правку кадра</button><button type="button" class="btn ghost sm" disabled={locked||!!draft||!future.length} onclick={redo}>Повторить правку кадра</button><button type="button" class="btn ghost sm" disabled={locked||!!draft} onclick={()=>commit({custom:false,strokes:[],feather:0})}>Сбросить выбранный кадр</button></div>
 <button type="button" class="btn ghost sm" disabled={locked||!!draft||!dirty} onclick={()=>{edits=JSON.parse(baseline);past=[];future=[]}}>Отбросить правки видео</button>
 <button type="button" class="btn ghost sm" disabled={locked||!!draft||!dirty} onclick={compute}>Пересчитать видеомаски</button>
 </fieldset>{/if}
 {#if saving}<p role="status">Создаю задание правки…</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
</details>
<style>
 fieldset{border:0;padding:0;min-width:0}label{display:flex;flex-direction:column;gap:.25rem;margin-block:.5rem}select,input{max-width:100%;min-width:0}.actions{display:flex;flex-wrap:wrap;gap:.25rem}.canvas{padding:0;border:0;line-height:0;display:block;touch-action:none;background:var(--surface)}.canvas svg{display:block;width:100%;height:auto}p{overflow-wrap:anywhere}
</style>
