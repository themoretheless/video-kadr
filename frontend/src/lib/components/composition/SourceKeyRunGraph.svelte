<script lang="ts">
 import {sourceRunDelta,type SourceRunEdit} from '$lib/composition/sourceRunSelection'
 import {tick} from 'svelte'
 import {createSourceRunCutTransaction,withSourceRunCutTransaction} from '$lib/composition/sourceRunCutTransaction'
 import {captureSourceRunClipboard,pasteSourceRunClipboard,parseSourceRunClipboard,serializeSourceRunClipboard,type SourceRunClipboard} from '$lib/composition/sourceRunClipboard'
 import {sourceRunClipboard} from '$lib/state/sourceRunClipboard.svelte'
 import type {NativeNestedProject} from '$lib/api'
 import type {WireConcatParamKey} from '$lib/composition/types'
 import {prepareConcatSourceRun} from '$lib/composition/concatSourceWindow'
 import {nestedComposition,nestedAgentGeneration} from '$lib/state/nestedComposition.svelte'
 import {authState} from '$lib/state/auth.svelte'
 import {agentSession} from '$lib/agent/session.svelte'
 let {keys,label,identity,disabled=false,rest=0,emptyRange=[0,1],onchange,oninsert,onremove,onbatch,onremoveBatch,onreplace}:{keys:readonly WireConcatParamKey[];label:string;identity:string;disabled?:boolean;rest?:number;emptyRange?:readonly[number,number];onchange:(index:number,change:{at?:number;value?:number},expected:string)=>void;oninsert?:(key:{at:number;value:number},expected:string)=>void;onremove?:(index:number,expected:string)=>void;onbatch?:(edits:readonly SourceRunEdit[],expected:string)=>void;onremoveBatch?:(indices:readonly number[],expected:string)=>void;onreplace?:(keys:readonly WireConcatParamKey[],expected:string)=>void}=$props()
 type Range={scale:number;low:number;high:number}
 type Gesture={project:NativeNestedProject|null;projectSignature:string;pointer:number;target:SVGElement;rect:DOMRect;index:number;indices:number[];x:number;y:number;at:number;value:number;identity:string;signature:string;context:string;range:Range;draft:{at:number;value:number}}
 let gesture=$state.raw<Gesture|null>(null),selection=$state.raw<number[]>([])
 let alive=true
 let selectionContext='',selectionSignature='',pendingSelection:{signature:string;context:string;indices:number[]}|null=null
 const coordinates=(run:readonly WireConcatParamKey[])=>JSON.stringify(run.map(key=>[key.at,key.value]))
 let pasteAt=$state<number|undefined>(.5),message=$state(''),pendingCut=$state.raw<{base:string;expected:string;context:string;clipboard:SourceRunClipboard;busy:boolean;native:boolean}|null>(null)
 const blocked=$derived(disabled||!!agentSession.lock||!!pendingCut)
 const shown=$derived(gesture?keys.map((key,index)=>gesture!.indices.includes(index)?{...key,at:key.at+(gesture!.draft.at-gesture!.at),value:key.value+(gesture!.draft.value-gesture!.value)}:key):keys)
 const samples=$derived.by(()=>{if(!shown.length)return Number.isFinite(rest)?[{at:0,value:rest},{at:1,value:rest}]:[];const sample=prepareConcatSourceRun(shown);return Array.from({length:257},(_,index)=>({at:index/256,value:sample(index/256)})).filter(point=>Number.isFinite(point.value))})
 const automatic=$derived.by(()=>{const values=shown.length?[...shown,...samples]:emptyRange.filter(Number.isFinite).map(value=>({value}));let scale=1;for(const key of values)scale=Math.max(scale,Math.abs(key.value));let low=Infinity,high=-Infinity;for(const key of values){low=Math.min(low,key.value/scale);high=Math.max(high,key.value/scale)}if(!Number.isFinite(low))return {scale:1,low:0,high:1};const padding=!shown.length&&high>low?0:Math.max((high-low)*.1,.1/scale,Number.EPSILON*16);return {scale,low:low-padding,high:high+padding}})
 const range=$derived(gesture?.range??automatic)
 const x=(at:number)=>20+Math.max(0,Math.min(1,at))*600
 const y=(value:number)=>10+(range.high-value/range.scale)/(range.high-range.low)*160
 const context=()=>JSON.stringify([nestedComposition.projectId,nestedComposition.activeCompositionId,nestedAgentGeneration(),authState.token,nestedComposition.timeUs])
 function cancel(){const old=gesture;gesture=null;if(old?.target.hasPointerCapture?.(old.pointer))old.target.releasePointerCapture(old.pointer)}
 function valid(held:Gesture){const rect=held.target.ownerSVGElement?.getBoundingClientRect();return !blocked&&identity===held.identity&&JSON.stringify(keys)===held.signature&&context()===held.context&&nestedComposition.document===held.project&&JSON.stringify(nestedComposition.document)===held.projectSignature&&held.target.isConnected&&!!rect&&(['left','top','width','height'] as const).every(key=>Math.abs(rect[key]-held.rect[key])<.5)}
 $effect(()=>{if(gesture&&(blocked||identity!==gesture.identity||JSON.stringify(keys)!==gesture.signature||context()!==gesture.context||nestedComposition.document!==gesture.project||JSON.stringify(nestedComposition.document)!==gesture.projectSignature))cancel()})
 $effect(()=>{const nextContext=context(),signature=JSON.stringify(keys);if(nextContext!==selectionContext){selectionContext=nextContext;selectionSignature=signature;selection=[];pendingSelection=null}else if(signature!==selectionSignature){selectionSignature=signature;selection=pendingSelection?.context===nextContext&&pendingSelection.signature===coordinates(keys)?pendingSelection.indices.filter(index=>index<keys.length):[];pendingSelection=null}})
 $effect(()=>{const pending=pendingCut;if(!pending||pending.native)return;const signature=JSON.stringify(keys);if(context()!==pending.context||signature!==pending.base&&signature!==pending.expected){pendingCut=null}else if(signature===pending.expected){sourceRunClipboard.value=pending.clipboard;pendingCut=null}else if(disabled&&!pending.busy){pendingCut={...pending,busy:true}}else if(!disabled&&pending.busy){pendingCut=null}})
 $effect(()=>()=>{alive=false;cancel()})
 function down(event:PointerEvent,index:number){
  if(blocked||gesture||event.button!==0)return
  const target=event.currentTarget as SVGElement,rect=target.ownerSVGElement!.getBoundingClientRect(),key=keys[index]!
  if(!rect.width||!rect.height)return
  event.preventDefault();event.stopPropagation();target.focus();if(event.shiftKey||event.ctrlKey||event.metaKey){selection=selection.includes(index)?selection.filter(at=>at!==index):[...selection,index]}else if(!selection.includes(index))selection=[index];if(!selection.includes(index))return;target.setPointerCapture?.(event.pointerId)
  gesture={project:nestedComposition.document,projectSignature:JSON.stringify(nestedComposition.document),pointer:event.pointerId,target,rect,index,indices:[...selection],x:event.clientX,y:event.clientY,at:key.at,value:key.value,identity,signature:JSON.stringify(keys),context:context(),range:{...range},draft:{at:key.at,value:key.value}}
 }
 function move(event:PointerEvent){
  const held=gesture;if(!held||event.pointerId!==held.pointer)return
  if(!valid(held)){cancel();return}
  try{
  const edits=sourceRunDelta(keys,held.indices,(event.clientX-held.x)/held.rect.width*640/600,-(event.clientY-held.y)/held.rect.height*180/160*(held.range.high-held.range.low)*held.range.scale)
  const anchor=edits.find(edit=>edit.index===held.index)?.change
  event.preventDefault();gesture={...held,draft:{at:anchor?.at??held.at,value:anchor?.value??held.value}}
  }catch{cancel()}

 }
 function applyEdits(edits:readonly SourceRunEdit[],expected:string){
  if(!edits.length)return
  const next=keys.map((key,index)=>({...key,...edits.find(edit=>edit.index===index)?.change}))
  pendingSelection={signature:coordinates(next),context:context(),indices:[...selection]}
  if(edits.length>1&&onbatch)onbatch(edits,expected);else if(edits.length===1)onchange(edits[0]!.index,edits[0]!.change,expected)
 }
 function up(event:PointerEvent){const held=gesture;if(!held||held.pointer!==event.pointerId)return;move(event);const final=gesture;if(!final||!valid(final)){cancel();return}const edits=sourceRunDelta(keys,held.indices,final.draft.at-held.at,final.draft.value-held.value);cancel();applyEdits(edits,held.identity)}
 async function copy(cut=false){
  if(blocked)return
  try{
   const clipboard=captureSourceRunClipboard(keys,selection);message=''
   if(!cut){sourceRunClipboard.value=clipboard;return}
   if(!onreplace)return
   const run=keys.filter((_,index)=>!selection.includes(index)),pending={base:JSON.stringify(keys),expected:JSON.stringify(run),context:context(),clipboard,busy:false,native:false}
   const transaction=createSourceRunCutTransaction(clipboard,()=>{if(context()!==pending.context)throw new Error('Контекст вырезания исходных ключей изменился')})
   pendingCut=pending
   try{withSourceRunCutTransaction(transaction,()=>onreplace!(run,identity))}catch(error){transaction.settle(false);throw error}
   if(transaction.claimed){const native={...pending,native:true};pendingCut=native;await transaction.result;if(alive&&pendingCut===native)pendingCut=null}
   else{transaction.settle(false);await tick();if(pendingCut===pending&&!disabled&&JSON.stringify(keys)===pending.base)pendingCut=null}
  }catch(error){pendingCut=null;message=error instanceof Error?error.message:String(error)}
 }
 function paste(){
  if(blocked||!onreplace||!sourceRunClipboard.value)return
  try{const run=pasteSourceRunClipboard(keys,sourceRunClipboard.value,pasteAt??.5);message='';onreplace(run,identity)}catch(error){message=error instanceof Error?error.message:String(error)}
 }
 async function systemClipboard(read:boolean){
  if(blocked)return
  const signature=JSON.stringify(keys),expectedContext=context()
  try{if(read){const text=await navigator.clipboard.readText();if(!alive||blocked||context()!==expectedContext||JSON.stringify(keys)!==signature)return;sourceRunClipboard.value=parseSourceRunClipboard(text)}else if(sourceRunClipboard.value)await navigator.clipboard.writeText(serializeSourceRunClipboard(sourceRunClipboard.value));message=''}catch(error){message=error instanceof Error?error.message:String(error)}
 }
 function shortcut(event:KeyboardEvent){if(!(event.ctrlKey||event.metaKey)||!['c','x','v'].includes(event.key.toLowerCase()))return false;event.preventDefault();event.stopPropagation();if(event.key.toLowerCase()==='v')paste();else void copy(event.key.toLowerCase()==='x');return true}
 function insert(at:number,value:number){if(!blocked&&!gesture&&Number.isFinite(at)&&Number.isFinite(value))oninsert?.({at:Math.max(0,Math.min(1,at)),value},identity)}
 function doubleClick(event:MouseEvent){
  if(blocked||gesture||!oninsert||event.button!==0)return
  const rect=(event.currentTarget as SVGElement).ownerSVGElement!.getBoundingClientRect();if(!rect.width||!rect.height)return
  event.preventDefault();event.stopPropagation()
  insert(((event.clientX-rect.left)/rect.width*640-20)/600,(range.high-((event.clientY-rect.top)/rect.height*180-10)/160*(range.high-range.low))*range.scale)
 }
 function keyboard(event:KeyboardEvent,index:number){
  if(blocked||gesture)return
  if(shortcut(event))return
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='a'){event.preventDefault();event.stopPropagation();selection=keys.map((_,index)=>index);return}
  if(event.ctrlKey||event.metaKey)return
  if(event.key==='Delete'||event.key==='Backspace'){event.preventDefault();event.stopPropagation();const indices=selection.includes(index)?selection:[index];if(indices.length>1&&onremoveBatch)onremoveBatch(indices,identity);else onremove?.(index,identity);return}
  if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key))return
  event.preventDefault();event.stopPropagation();if(!selection.includes(index))selection=[index]
  const step=event.altKey ? .001 : event.shiftKey ? .1 : .01
  applyEdits(sourceRunDelta(keys,selection,event.key==='ArrowRight'?step:event.key==='ArrowLeft'?-step:0,(event.key==='ArrowUp'?step:event.key==='ArrowDown'?-step:0)*(range.high-range.low)*range.scale),identity)
 }

</script>
<svelte:window onresize={cancel} onkeydown={event=>{if(event.key==='Escape'&&gesture){event.preventDefault();cancel()}}} />
{#if keys.length||oninsert}
 <div class="source-run">
  <svg viewBox="0 0 640 180" preserveAspectRatio="none" role="group" aria-label={`${label} график исходных ключей`} onpointermove={move} onpointerup={up} onpointercancel={event=>{if(gesture?.pointer===event.pointerId)cancel()}}>
   <rect x="20" y="10" width="600" height="160" fill="transparent" role="button" tabindex={blocked||!oninsert?-1:0} aria-disabled={blocked||!oninsert} aria-label={`${label} добавить исходный ключ`} ondblclick={doubleClick} onkeydown={event=>{if(shortcut(event))return;if(event.key==='Enter'||event.key===' '){event.preventDefault();event.stopPropagation();insert(.5,keys.length?prepareConcatSourceRun(keys)(.5):rest)}}} />
   <path d={samples.map((point,index)=>`${index?'L':'M'}${x(point.at)},${y(point.value)}`).join(' ')} fill="none" stroke="currentColor" pointer-events="none" />
   {#each shown as key,index (index)}
    <circle role="button" tabindex={blocked?-1:0} aria-disabled={blocked} aria-pressed={selection.includes(index)} stroke={selection.includes(index)?'currentColor':'none'} stroke-width="2" aria-label={`${label} исходный ключ ${index}`} cx={x(key.at)} cy={y(key.value)} r="5" fill="var(--accent,#6af)" onpointerdown={event=>down(event,index)} onkeydown={event=>keyboard(event,index)} onlostpointercapture={event=>{if(gesture?.target===event.currentTarget)cancel()}}><title>{key.at} · {key.value}</title></circle>
   {/each}
  </svg>
  <div class="clipboard">
   <button type="button" class="btn ghost sm" disabled={blocked||!selection.length} onclick={()=>void copy()}>Копировать исходные ключи</button>
   <button type="button" class="btn ghost sm" disabled={blocked||!selection.length||!onreplace} onclick={()=>void copy(true)}>Вырезать исходные ключи</button>
   <label>Вставка, доля<input aria-label={`${label} позиция вставки исходных ключей`} type="number" min="0" max="1" step="any" bind:value={pasteAt} disabled={blocked} /></label>
   <button type="button" class="btn ghost sm" disabled={blocked||!sourceRunClipboard.value||!onreplace} onclick={paste}>Вставить исходные ключи</button>
   <button type="button" class="btn ghost sm" disabled={blocked} onclick={()=>void systemClipboard(true)}>Прочитать исходные ключи из системного буфера</button>
   <button type="button" class="btn ghost sm" disabled={blocked||!sourceRunClipboard.value} onclick={()=>void systemClipboard(false)}>Записать исходные ключи в системный буфер</button>
  </div>
  {#if message}<p role="alert">{message}</p>{/if}
  <small>Доля клипа 0–1 · двойной щелчок добавляет ключ · Shift/Ctrl/⌘ выбирают несколько · Ctrl/⌘A выбирает все · Delete удаляет выделение · точка меняет время и значение · стрелки, Shift ×10, Alt ×0,1 · Escape отменяет</small>
 </div>
{/if}
<style>.clipboard{display:flex;gap:.3rem;flex-wrap:wrap;align-items:center}.clipboard input{width:5rem}.source-run{width:min(100%,40rem)}svg{display:block;width:100%;height:180px;touch-action:none;overflow:hidden}circle{cursor:move}small{display:block;font-size:.75rem}</style>
