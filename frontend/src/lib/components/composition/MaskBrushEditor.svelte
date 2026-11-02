<script lang="ts">
 import {untrack} from 'svelte'
 import SamRegionTool from './SamRegionTool.svelte'
 import type {SavedSamRegion} from '$lib/api'
 import {isValidSamRegion} from '$lib/composition/validation'
 import {getSelfieOriginal,startSelfieCorrection,getProjectCutoutOriginal,getProjectCutoutSettings,startProjectCutoutCorrection,type ProjectCutoutContext,getCutoutOriginal,getCutoutCorrection,startCutoutCorrection,type MaskStroke,type MaskCorrection} from '$lib/api'
 let {token,jobId,width,height,projectMask,projectId=null,sourceId=null,sourceHash=null,jobFamily='object',disabled=false,onready,onchanged}:{token:string;jobId:string;projectMask?:ProjectCutoutContext;jobFamily?:'object'|'selfie';projectId?:string|null;sourceId?:string|null;sourceHash?:string|null;width:number;height:number;disabled?:boolean;onready:(id:string)=>void;onchanged:(dirty:boolean)=>void}=$props()
 let opened=$state(false),loading=$state(false),saving=$state(false),error=$state(''),original=$state(''),loaded=$state(false)
 let strokes=$state<MaskStroke[]>([]),redo=$state<MaskStroke[]>([]),feather=$state(0),custom=$state(false),tool=$state<MaskStroke['tool']>('brush'),size=$state(0.08)
 let draft=$state<MaskStroke|null>(null),pointer:number|undefined,baseline=$state(''),cursor=$state<[number,number]>([0.5,0.5])
 let learning=$state(false)
 const lastSmart=$derived(strokes.at(-1)?.tool==='smartBrush'||strokes.at(-1)?.tool==='smartEraser'?strokes.at(-1):undefined)
 let request:AbortController|undefined,ownedUrl=''
 const scope=$derived(JSON.stringify([token,jobId,width,height,projectMask,projectId,sourceId,sourceHash,jobFamily]))
 const recipe=$derived<MaskCorrection>({custom,strokes,feather})
 const dirty=$derived(loaded&&(Boolean(draft)||canonical(recipe)!==baseline))
 const locked=$derived(disabled||loading||saving||learning||!loaded)
 const count=$derived(strokes.reduce((n,s)=>n+s.points.length,0))
 const canvasWidth=$derived(Math.min(480,360*width/height))
 const current=(owner:string,controller:AbortController)=>scope===owner&&!controller.signal.aborted
 function clear(){if(ownedUrl)URL.revokeObjectURL(ownedUrl);ownedUrl='';original=''}
 $effect(()=>{const owner=scope;untrack(()=>{request?.abort();clear();opened=false;loaded=false;loading=false;saving=false;error='';strokes=[];redo=[];feather=0;custom=false;draft=null;pointer=undefined;baseline='';learning=false});return()=>{void owner;request?.abort();clear();untrack(()=>onchanged(false))}})
 $effect(()=>{const changed=dirty||learning;untrack(()=>onchanged(changed))})
 function canonical(value:MaskCorrection){return JSON.stringify({custom:value.custom,strokes:value.strokes.map(s=>({tool:s.tool,size:s.size,points:s.points,at:s.at??null,...(s.region?{region:{sourceId:s.region.sourceId,maskSourceId:s.region.maskSourceId,sourceHash:s.region.sourceHash,maskHash:s.region.maskHash,maskFileHash:s.region.maskFileHash,encoderHash:s.region.encoderHash,decoderHash:s.region.decoderHash,runtimeHash:s.region.runtimeHash,promptHash:s.region.promptHash,points:s.region.points}}:{})})),feather:value.feather})}
 function attachmentKey(value:ProjectCutoutContext['snapshot']){return JSON.stringify([value.sourceId,value.maskSourceId,value.sourceHash,value.maskHash,value.maskFileHash,value.modelHash,value.runtimeHash,value.subject??null,canonical(value.correction)])}
 function validate(value:MaskCorrection){if(typeof value?.custom!=='boolean'||!Number.isFinite(value.feather)||value.feather<0||value.feather>0.1||!Array.isArray(value.strokes)||value.strokes.length>1024)throw new Error('Неверные сохранённые правки');let total=0;for(const s of value.strokes){total+=s.points?.length??0;if(!['brush','eraser','smartBrush','smartEraser'].includes(s.tool)||!Number.isFinite(s.size)||s.size<0.005||s.size>0.5||!Array.isArray(s.points)||s.points.length===0||total>65536||!s.points.every(p=>p.length===2&&p.every(v=>typeof v==='number'&&Number.isFinite(v)&&v>=-0.5&&v<=1.5))||(s.at!=null&&(!Number.isFinite(s.at)||s.at<0)))throw new Error('Неверные сохранённые штрихи');if(s.region&&(!['smartBrush','smartEraser'].includes(s.tool)||!isValidSamRegion(s.region,projectMask?.snapshot.sourceId,projectMask?.snapshot.sourceHash,s.points)))throw new Error('Неверная сохранённая область')}}
 async function load(){if(disabled||loading||saving||learning)return;const owner=scope,controller=new AbortController();request?.abort();request=controller;loading=true;error='';try{const [blob,settings]=projectMask?await Promise.all([getProjectCutoutOriginal(token,projectMask,controller.signal),getProjectCutoutSettings(token,projectMask,controller.signal).then(result=>{if(attachmentKey(result.snapshot)!==attachmentKey(projectMask!.snapshot)||result.width!==width||result.height!==height)throw new Error('Сохранённая маска изменилась. Сохраните проект и откройте полотно заново');return {correction:result.snapshot.correction}})]):await Promise.all([(jobFamily==='selfie'?getSelfieOriginal:getCutoutOriginal)(token,jobId,controller.signal),getCutoutCorrection(token,jobId,controller.signal)]);if(!current(owner,controller))return;const value=settings?.correction??{custom:false,strokes:[],feather:0};validate(value);clear();ownedUrl=URL.createObjectURL(blob);original=ownedUrl;strokes=JSON.parse(JSON.stringify(value.strokes));feather=value.feather;custom=value.custom;baseline=canonical(value);redo=[];loaded=true}catch(reason){if(current(owner,controller))error=reason instanceof Error?reason.message:String(reason)}finally{if(current(owner,controller))loading=false}}
 function point(event:PointerEvent):[number,number]{const rect=(event.currentTarget as HTMLElement).getBoundingClientRect();if(rect.width<=0||rect.height<=0)throw new Error('Полотно ещё не открыто');return [Math.max(-0.5,Math.min(1.5,(event.clientX-rect.left)/rect.width)),Math.max(-0.5,Math.min(1.5,(event.clientY-rect.top)/rect.height))]}
 function begin(event:PointerEvent){if(locked||event.button!==0||pointer!==undefined)return;if(strokes.length>=1024||count>=65536){error='Достигнут предел штрихов. Удалите часть правок.';return}try{const p=point(event);cursor=p;draft={tool,size,points:[p],at:null};pointer=event.pointerId;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);event.preventDefault()}catch(reason){error=String(reason);draft=null;pointer=undefined}}
 function move(event:PointerEvent){if(locked||pointer!==event.pointerId||!draft)return;const p=point(event),last=draft.points.at(-1)!;cursor=p;if(Math.hypot(p[0]-last[0],p[1]-last[1])<0.002)return;if(count+draft.points.length>=65536){error='Достигнут предел точек. Завершите штрих.';return}draft={...draft,points:[...draft.points,p]}}
 function finish(event:PointerEvent){if(pointer!==event.pointerId)return;if(!locked&&draft){move(event);strokes=[...strokes,draft];custom=true;redo=[]}draft=null;pointer=undefined;const canvas=event.currentTarget as HTMLElement;if(canvas.hasPointerCapture?.(event.pointerId))canvas.releasePointerCapture(event.pointerId)}
 function cancelPointer(){draft=null;pointer=undefined}
 function keyboard(event:KeyboardEvent){if(locked||pointer!==undefined)return;const step=event.shiftKey?0.1:0.01;const direction:Record<string,[number,number]>={ArrowLeft:[-step,0],ArrowRight:[step,0],ArrowUp:[0,-step],ArrowDown:[0,step]};const d=direction[event.key];if(d){event.preventDefault();cursor=[Math.max(0,Math.min(1,cursor[0]+d[0])),Math.max(0,Math.min(1,cursor[1]+d[1]))]}else if(event.key==='Enter'||event.key===' '){event.preventDefault();if(event.repeat)return;if(strokes.length>=1024||count>=65536){error='Достигнут предел штрихов';return}strokes=[...strokes,{tool,size,points:[[...cursor]],at:null}];custom=true;redo=[]}}
 function undo(){if(locked||draft||!strokes.length)return;redo=[...redo,strokes.at(-1)!];strokes=strokes.slice(0,-1);const loaded=JSON.parse(baseline) as MaskCorrection;if(canonical({custom:false,strokes,feather:0})===canonical({custom:false,strokes:loaded.strokes,feather:0}))custom=loaded.custom}
 function repeat(){if(locked||draft||!redo.length)return;strokes=[...strokes,redo.at(-1)!];redo=redo.slice(0,-1);custom=true}
 function discard(){if(locked)return;const value=JSON.parse(baseline) as MaskCorrection;strokes=value.strokes;custom=value.custom;feather=value.feather;redo=[];draft=null;pointer=undefined}
 function automatic(){if(locked||draft)return;strokes=[];redo=[];feather=0;custom=false}
 function path(stroke:MaskStroke){return stroke.points.map(([x,y],i)=>`${i?'L':'M'}${x*width} ${y*height}`).join(' ')+(stroke.points.length===1?` L${stroke.points[0]![0]*width} ${stroke.points[0]![1]*height}`:'')}
 async function attachRegion(entry:SavedSamRegion){
  const source=projectMask?.snapshot.sourceId??sourceId,hash=projectMask?.snapshot.sourceHash??sourceHash;
  if(!source||!hash||!isValidSamRegion(entry.region,source,hash))throw new Error('Область относится к другому исходнику');
  const region=JSON.parse(JSON.stringify(entry.region)) as NonNullable<MaskStroke['region']>,last=strokes.at(-1);
  if(last&&(last.tool==='smartBrush'||last.tool==='smartEraser')&&JSON.stringify(last.points)===JSON.stringify(region.points)){strokes=[...strokes.slice(0,-1),{...last,region}]}
  else{if(tool!=='smartBrush'&&tool!=='smartEraser')throw new Error('Выберите умный инструмент');if(strokes.length>=1024||count+region.points.length>65536)throw new Error('Достигнут предел штрихов или точек');strokes=[...strokes,{tool,size,points:JSON.parse(JSON.stringify(region.points)),at:null,region}]}
  custom=true;redo=[]
 }
 async function compute(){if(locked||draft||!dirty)return;const owner=scope,controller=new AbortController(),captured=JSON.parse(JSON.stringify(recipe)) as MaskCorrection;request?.abort();request=controller;saving=true;error='';try{validate(captured);const result=projectMask?await startProjectCutoutCorrection(token,projectMask.projectId,projectMask.clipId,captured,projectMask.compositionId,controller.signal,projectMask.snapshot):await (jobFamily==='selfie'?startSelfieCorrection:startCutoutCorrection)(token,jobId,captured,controller.signal);if(!current(owner,controller))return;if(!result.jobId)throw new Error('Сервер не вернул ID правки');onready(result.jobId)}catch(reason){if(current(owner,controller))error=reason instanceof Error?reason.message:String(reason)}finally{if(current(owner,controller))saving=false}}
</script>
<details bind:open={opened}><summary>Правка маски</summary>
 <p>Рисуйте по исходному изображению. Цветные линии показывают штрихи; «Пересчитать маску» создаст новый прозрачный результат.</p>
 <button type="button" class="btn ghost sm" disabled={disabled||loading||saving||learning||dirty} onclick={load}>Открыть полотно маски</button>
 {#if loading}<p role="status">Открываю исходник и сохранённые правки…</p>{/if}
 {#if loaded}
 <fieldset disabled={locked}>
 <label>Инструмент<select aria-label="Инструмент маски" value={tool} onchange={event=>{const value=event.currentTarget.value;if(value==='brush'||value==='eraser'||value==='smartBrush'||value==='smartEraser')tool=value}}><option value="brush">Сохранить</option><option value="eraser">Удалить</option><option value="smartBrush">Сохранить вероятный объект</option><option value="smartEraser">Убрать сомнительное</option></select></label>
 <label>Диаметр кисти<input aria-label="Диаметр кисти маски" type="range" min="0.005" max="0.5" step="0.005" bind:value={size}/>{Math.round(size*100)}%</label>
 <label>Растушёвка<input aria-label="Растушёвка маски" type="range" min="0" max="0.1" step="0.001" bind:value={feather}/>{(feather*100).toFixed(1)}%</label>
 <p>Умные инструменты используют исходную маску. В разделе SAM можно выделить объект целиком. Стрелки перемещают кисть, Enter ставит точку.</p>
 <button type="button" class="canvas" aria-label="Полотно правки маски" style:width={`min(100%, ${canvasWidth}px)`} onpointerdown={begin} onpointermove={move} onpointerup={finish} onpointercancel={cancelPointer} onlostpointercapture={cancelPointer} onkeydown={keyboard}>
 <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
 <title>Рисование штрихов маски по исходному изображению</title>
 <image href={original} x="0" y="0" {width} {height} preserveAspectRatio="none"/>
 {#each (custom?strokes:[]) as stroke,i (i)}<path d={path(stroke)} stroke={stroke.tool==='eraser'||stroke.tool==='smartEraser'?'var(--danger)':'var(--ok)'} stroke-width={stroke.size*width} stroke-linecap="round" stroke-linejoin="round" fill="none" opacity="0.65"/>{/each}
 {#if draft}<path d={path(draft)} stroke="var(--accent)" stroke-width={draft.size*width} stroke-linecap="round" stroke-linejoin="round" fill="none" opacity="0.65"/>{/if}
 <circle cx={cursor[0]*width} cy={cursor[1]*height} r={size*width/2} fill="none" stroke="var(--text)" stroke-width={width/480}/>
 </svg></button>
 <p role="status">Штрихов: {strokes.length}. Областей SAM: {strokes.filter(s=>s.region).length}{#if dirty}. Правки ещё не пересчитаны.{/if}</p>
 <div class="actions"><button type="button" class="btn ghost sm" disabled={locked||!!draft||!strokes.length} onclick={undo}>Отменить штрих</button><button type="button" class="btn ghost sm" disabled={locked||!!draft||!redo.length} onclick={repeat}>Повторить штрих</button><button type="button" class="btn ghost sm" disabled={locked||!!draft} onclick={automatic}>Вернуть автоматическую маску</button></div>
 <button type="button" class="btn ghost sm" disabled={locked||!!draft||!dirty} onclick={discard}>Отбросить несохранённые правки</button>
 <button type="button" class="btn ghost sm" disabled={locked||!!draft||!dirty} onclick={compute}>Пересчитать маску</button>
 </fieldset>
 <SamRegionTool {token} projectId={projectMask?.projectId??projectId} sourceId={projectMask?.snapshot.sourceId??sourceId} sourceHash={projectMask?.snapshot.sourceHash??sourceHash} targetKey={scope} points={lastSmart?.points} canAttach={tool==='smartBrush'||tool==='smartEraser'} disabled={disabled||loading||saving||!!draft} onbusy={value=>{learning=value}} onready={attachRegion}/>

 {/if}
 {#if saving}<p role="status">Создаю задание правки…</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
</details>
<style>
 fieldset{border:0;padding:0;min-width:0}label{display:flex;flex-direction:column;gap:.25rem;margin-block:.5rem}select,input{max-width:100%;min-width:0}.actions{display:flex;flex-wrap:wrap;gap:.25rem}.canvas{padding:0;border:0;line-height:0;display:block;touch-action:none;background-color:var(--surface);background-image:repeating-conic-gradient(var(--border) 0% 25%,transparent 0% 50%);background-size:20px 20px}.canvas svg{display:block;width:100%;height:auto}p{overflow-wrap:anywhere}
</style>
