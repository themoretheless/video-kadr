<script lang="ts">
 import type {EasePoints} from '$lib/composition/concatRadiusSourceKeys'
 import {graphBox,graphRange,graphPoint,clientGraphPoint,replaceGraphPoint,sameGraphPoints,type GraphRange} from '$lib/composition/sourceBezierGraph'
 let {points,label,identity='',disabled=false,onchange}:{points:EasePoints;label:string;identity?:string;disabled?:boolean;onchange:(points:EasePoints,expectedIdentity:string)=>void}=$props()
 let svg:SVGSVGElement|undefined=$state()
 type Gesture={pointer:number;index:0|1;base:EasePoints;draft:EasePoints;identity:string;range:GraphRange;rect:{left:number;top:number;width:number;height:number};offset:readonly[number,number];target:Element}
 let gesture=$state<Gesture|null>(null)
 let current=$derived(gesture?.draft??points)
 let range=$derived(gesture?.range??graphRange(points))
 let start=$derived(graphPoint(0,0,range)),end=$derived(graphPoint(1,1,range))
 let first=$derived(graphPoint(current[0],current[1],range)),second=$derived(graphPoint(current[2],current[3],range))
 function cancel(){const old=gesture;gesture=null;if(old?.target.hasPointerCapture?.(old.pointer))old.target.releasePointerCapture(old.pointer)}
 $effect(()=>{if(gesture&&(disabled||identity!==gesture.identity||!sameGraphPoints(points,gesture.base)))cancel()})
 $effect(()=>()=>cancel())
 function down(event:PointerEvent,index:0|1){
  if(disabled||gesture||event.button!==0||!event.isPrimary||!svg)return
  const rect=svg.getBoundingClientRect(),range=graphRange(points),at=clientGraphPoint(event.clientX,event.clientY,rect,range);if(!at)return
  event.preventDefault();const target=event.currentTarget as SVGCircleElement;target.focus({preventScroll:true});target.setPointerCapture?.(event.pointerId)
  gesture={pointer:event.pointerId,index,base:[...points] as unknown as EasePoints,draft:[...points] as unknown as EasePoints,identity,range,rect:{left:rect.left,top:rect.top,width:rect.width,height:rect.height},offset:[points[index*2]!-at[0],points[index*2+1]!-at[1]],target}
 }
 function move(event:PointerEvent){if(!gesture||event.pointerId!==gesture.pointer)return;const at=clientGraphPoint(event.clientX,event.clientY,gesture.rect,gesture.range);if(at)gesture.draft=replaceGraphPoint(gesture.base,gesture.index,at[0]+gesture.offset[0],at[1]+gesture.offset[1])}
 function up(event:PointerEvent){if(!gesture||event.pointerId!==gesture.pointer)return;move(event);const old=gesture;cancel();if(!disabled&&identity===old.identity&&sameGraphPoints(points,old.base)&&!sameGraphPoints(old.base,old.draft))onchange(old.draft,old.identity)}
 function keyboard(event:KeyboardEvent,index:0|1){
  if(event.key==='Escape'&&gesture){event.preventDefault();cancel();return}
  if(disabled||event.ctrlKey||event.metaKey||!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key))return
  event.preventDefault();if(gesture)return;const step=event.altKey?0.001:event.shiftKey?0.1:0.01
  const x=points[index*2]!+(event.key==='ArrowRight'?step:event.key==='ArrowLeft'?-step:0)
  const y=points[index*2+1]!+(event.key==='ArrowUp'?step:event.key==='ArrowDown'?-step:0)
  const next=replaceGraphPoint(points,index,x,y);if(!sameGraphPoints(next,points))onchange(next,identity)
 }
</script>
<div class="curve">
 <svg bind:this={svg} viewBox={`0 0 ${graphBox.width} ${graphBox.height}`} role="group" aria-label={`${label} Безье`}>
  <rect x="0" y="0" width={graphBox.width} height={graphBox.height} rx="6" class="background" />
  {#each [0,0.25,0.5,0.75,1] as tick}
   {@const p=graphPoint(tick,tick,range)}
   <line x1={p[0]} y1={graphBox.top} x2={p[0]} y2={graphBox.top+graphBox.plotHeight} class="grid" />
   <line x1={graphBox.left} y1={p[1]} x2={graphBox.left+graphBox.plotWidth} y2={p[1]} class="grid" />
  {/each}
  <line x1={start[0]} y1={start[1]} x2={first[0]} y2={first[1]} class="arm" />
  <line x1={end[0]} y1={end[1]} x2={second[0]} y2={second[1]} class="arm" />
  <path d={`M${start[0]},${start[1]} C${first[0]},${first[1]} ${second[0]},${second[1]} ${end[0]},${end[1]}`} class="path" />
  {#each [first,second] as point,index}
   <circle cx={point[0]} cy={point[1]} r="7" class:p2={index===1} class="handle" />
   <circle cx={point[0]} cy={point[1]} r="13" class="hit" role="button" tabindex={disabled?-1:0} aria-disabled={disabled} aria-label={`${label} P${index+1}`} onpointerdown={event=>down(event,index as 0|1)} onpointermove={move} onpointerup={up} onpointercancel={event=>{if(gesture?.pointer===event.pointerId)cancel()}} onlostpointercapture={event=>{if(gesture?.pointer===event.pointerId)cancel()}} onkeydown={event=>keyboard(event,index as 0|1)} />
  {/each}
 </svg>
 <small>Точки: мышью или стрелками · Shift ×10 · Alt ×0,1 · Esc отменяет жест</small>
 <output aria-live="polite">P₁ {current[0].toFixed(3)}, {current[1].toFixed(3)} · P₂ {current[2].toFixed(3)}, {current[3].toFixed(3)}</output>
</div>
<style>
 .curve {width:min(100%,20rem);min-width:12rem;} svg {width:100%;display:block;aspect-ratio:280/180;touch-action:none;user-select:none;}
 .background {fill:#171c27;} .grid {stroke:#394253;stroke-width:0.7;} .arm {stroke:#8594ad;stroke-dasharray:3 3;}
 .path {fill:none;stroke:#c4a7ff;stroke-width:2;} .handle {fill:#78bfff;stroke:#111827;stroke-width:2;pointer-events:none;}.handle.p2 {fill:#ffbd78;}
 .hit {fill:transparent;cursor:grab;} .hit:focus {stroke:#fff;stroke-width:2;outline:none;}.hit[aria-disabled=true] {cursor:default;}
 small,output {display:block;font-size:0.75rem;}
</style>
