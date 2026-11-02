<script lang="ts">
 import {nestedComposition,nestedPlayback,moveNestedConcatKeyMarks} from '$lib/state/nestedComposition.svelte'
 import type {NativeNestedProject} from '$lib/api'
 let {marks,disabled=false,compositionId,clipIds=[],rangeStart,rangeEnd}:{marks:readonly {timeUs:number;count:number}[];disabled?:boolean;compositionId?:string;clipIds?:readonly string[];rangeStart?:number;rangeEnd?:number}=$props()
 let first=$derived(rangeStart??marks[0]?.timeUs??0),last=$derived(rangeEnd??marks.at(-1)?.timeUs??first)
 type Drag={pointer:number;x:number;width:number;from:number;to:number;first:number;last:number;scope:string;ids:string[];document:NativeNestedProject;moved:boolean}
 let drag= $state.raw<Drag|undefined>(),busy=$state(false),error=$state(''),suppressClick=false
 function seek(time:number){if(suppressClick){suppressClick=false;return}if(disabled||busy)return;nestedPlayback.playing=false;nestedPlayback.epoch++;nestedComposition.timeUs=time}
 function down(event:PointerEvent,time:number){
  if(event.button!==0||disabled||busy||!compositionId||!clipIds.length||!nestedComposition.document||last<=first)return
  const element=event.currentTarget as HTMLButtonElement,width=element.parentElement!.getBoundingClientRect().width
  if(width<=0)return
  suppressClick=false;error='';drag={pointer:event.pointerId,x:event.clientX,width,from:time,to:time,first,last,scope:compositionId,ids:[...clipIds],document:nestedComposition.document,moved:false};element.setPointerCapture(event.pointerId)
 }
 function move(event:PointerEvent){
  if(!drag||drag.pointer!==event.pointerId)return
  const delta=event.clientX-drag.x,moved=drag.moved||Math.abs(delta)>3
  if(!moved)return
  event.preventDefault();drag={...drag,moved,to:Math.round(Math.max(drag.first,Math.min(drag.last,drag.from+delta*(drag.last-drag.first)/drag.width)))}
 }
 async function up(event:PointerEvent){
  const held=drag;if(!held||held.pointer!==event.pointerId)return
  drag=undefined
  if(!held.moved)return
  suppressClick=true
  if(disabled||held.scope!==compositionId||held.ids.join('\0')!==clipIds.join('\0')||held.document!==nestedComposition.document)return
  busy=true
  try{await moveNestedConcatKeyMarks(held.scope,held.ids,held.from,held.to,held.document)}catch(cause){error=cause instanceof Error?cause.message:'Не удалось переместить ключи'}finally{busy=false}
 }
 function cancel(){drag=undefined}
 function shown(time:number){return drag?.from===time?drag.to:time}
</script>
{#if marks.length}
 <div class="source-keys" role="group" aria-label="Исходные ключи выбранных клипов">
  <span>Исходные ключи</span>
  <div class="key-ruler">
   {#each marks as mark(mark.timeUs)}
    <button type="button" class="key-mark" disabled={disabled||busy} style:left={`${last===first?50:100*(shown(mark.timeUs)-first)/(last-first)}%`} aria-label={`Исходный ключ ${mark.timeUs} мкс${mark.count>1?` (${mark.count})`:''}`} title={`${(shown(mark.timeUs)/1000000).toFixed(3)} с${mark.count>1?` · ключей: ${mark.count}`:''}`} onclick={()=>seek(mark.timeUs)} onpointerdown={event=>down(event,mark.timeUs)} onpointermove={move} onpointerup={event=>void up(event)} onpointercancel={cancel} onlostpointercapture={cancel} onkeydown={event=>{if(event.key==='Escape')cancel()}}>◆</button>
   {/each}
  </div>
  <span>{(first/1000000).toFixed(3)}–{(last/1000000).toFixed(3)} с</span>
  {#if drag?.moved}<span role="status">Ключ: {(drag.to/1000000).toFixed(3)} с</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
 </div>
{/if}
<style>
 .source-keys{display:flex;align-items:center;gap:0.5rem;font-size:0.8rem;min-width:0;flex-wrap:wrap}
 .key-ruler{position:relative;height:2rem;flex:1;min-width:8rem;margin:0 0.75rem;border-bottom:1px solid currentColor}
 .key-mark{position:absolute;top:0;transform:translateX(-50%);border:0;background:transparent;color:inherit;padding:0.25rem;cursor:pointer;touch-action:none}
 .key-mark:disabled{opacity:0.4;cursor:default}
</style>
