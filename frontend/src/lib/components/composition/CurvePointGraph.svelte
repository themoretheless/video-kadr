<script module lang="ts">
 import type{CurvePoints}from'$lib/composition/concatCurveAuthoring'
 export interface CurveGesture {scope:unknown;clock:number;session:string;context:string;gesture:object}
 export interface CurvePointDraft extends CurveGesture {points:CurvePoints}
</script>
<script lang="ts">
 import{untrack}from'svelte'
 import{addCurvePoint,moveCurvePoint,removeCurvePoint}from'$lib/composition/concatCurveAuthoring'
 let{points,shown,path,label,disabled=false,scope,clock=0,session='',context='',tint='#a8c8ff',ghosts=[],onchange,onpreview}:{points:CurvePoints;shown?:CurvePoints;path:string;label:string;disabled?:boolean;scope:unknown;clock?:number;session?:string;context?:string;tint?:string;ghosts?:readonly{path:string;tint:string}[];onchange:(points:CurvePoints,meta:CurveGesture)=>void;onpreview?:(draft:CurvePointDraft|null,gesture:object)=>void}=$props()
 type Drag=CurvePointDraft&{id:number;index:number;base:CurvePoints;origin:readonly[number,number];off:boolean}
 let drag:Drag|undefined=$state.raw();let plot:SVGSVGElement;const help=$props.id()
 let visible=$derived(drag?.points??shown??points)
 function meta():CurveGesture{return{scope,clock,session,context,gesture:{}}}
 function same(d:CurveGesture){return d.scope===scope&&d.clock===clock&&d.session===session&&d.context===context}
 function coordinates(event:MouseEvent):[number,number]|undefined{
  const matrix=plot?.getScreenCTM?.();if(matrix&&typeof DOMPoint!=='undefined'){const p=new DOMPoint(event.clientX,event.clientY).matrixTransform(matrix.inverse());return[p.x/200,1-p.y/200]}
  const r=plot?.getBoundingClientRect();if(!r||r.width<=0||r.height<=0)return;return[(event.clientX-r.left)/r.width,1-(event.clientY-r.top)/r.height]
 }
 function drop(){if(!drag)return;const old=drag;drag=undefined;onpreview?.(null,old.gesture)}
 $effect(()=>{const base=points;const source=scope;const time=clock;const account=session;const target=context;if(drag&&(disabled||drag.base!==base||drag.scope!==source||drag.clock!==time||drag.session!==account||drag.context!==target))untrack(drop)})
 $effect(()=>()=>drop())
 function down(event:PointerEvent,index:number){if(disabled||event.button!==0||drag)return;const origin=coordinates(event);if(!origin)return;event.preventDefault();event.stopPropagation();drag={...meta(),points,base:points,origin,id:event.pointerId,index,off:false};const target=event.currentTarget as SVGCircleElement;target.focus({preventScroll:true});target.setPointerCapture(event.pointerId);onpreview?.(drag,drag.gesture)}
 function update(event:PointerEvent){if(!drag||drag.id!==event.pointerId)return;if(disabled||!same(drag)||drag.base!==points){drop();return}const p=coordinates(event);if(!p)return;const original=drag.base[drag.index]!;const x=original[0]+(p[0]-drag.origin[0]),y=original[1]+(p[1]-drag.origin[1]);drag={...drag,points:moveCurvePoint(drag.base,drag.index,x,y),off:x< -0.08||x>1.08||y< -0.08||y>1.08};onpreview?.(drag,drag.gesture)}
 function finish(event:PointerEvent){if(!drag||drag.id!==event.pointerId)return;update(event);const old=drag;if(!old)return;drag=undefined;if(!disabled&&same(old)&&old.base===points){const result=old.off?removeCurvePoint(old.base,old.index):old.points;if(JSON.stringify(result)!==JSON.stringify(points))onchange(result,old)}onpreview?.(null,old.gesture)}
 function cancel(event:PointerEvent){if(drag?.id===event.pointerId)drop()}
 function remove(index:number){if(!disabled)onchange(removeCurvePoint(points,index),meta())}
 function key(event:KeyboardEvent,index:number){if(event.key==='Escape'&&drag){event.preventDefault();event.stopPropagation();drop();return}if(disabled||drag||event.ctrlKey||event.altKey||event.metaKey)return;const p=points[index];if(!p)return;let[x,y]=p;const step=event.shiftKey?0.1:0.01;switch(event.key){case'ArrowLeft':x-=step;break;case'ArrowRight':x+=step;break;case'ArrowUp':y+=step;break;case'ArrowDown':y-=step;break;case'Delete':case'Backspace':event.preventDefault();event.stopPropagation();remove(index);return;default:return}event.preventDefault();event.stopPropagation();const result=moveCurvePoint(points,index,x,y);if(JSON.stringify(result)!==JSON.stringify(points))onchange(result,meta())}
 function add(event:MouseEvent){if(disabled)return;const p=coordinates(event);if(!p)return;const result=addCurvePoint(points,p[0],p[1]);if(result!==points)onchange(result,meta())}
 function backgroundKey(event:KeyboardEvent){if(disabled||!['Enter',' '].includes(event.key))return;event.preventDefault();event.stopPropagation();const result=addCurvePoint(points,0.5,0.5);if(result!==points)onchange(result,meta())}
</script>
<svg bind:this={plot} viewBox="0 0 200 200" role="group" aria-label={`Кривая ${label}`} aria-describedby={help}>
 <rect width="200" height="200" class="background" role="button" aria-label={`${label} добавить точку на графике`} aria-disabled={disabled||points.length>=8} tabindex={disabled||points.length>=8?-1:0} onclick={add} onkeydown={backgroundKey}/>
 <path d="M0 200L200 0" class="identity"/>
 {#each ghosts as ghost}<path d={ghost.path} class="ghost" stroke={ghost.tint}/>{/each}
 <path d={path} class="curve" stroke={tint}/>
 {#each visible as point,index}
  <circle cx={point[0]*200} cy={200-point[1]*200} r="6" fill={tint} class:off={drag?.index===index&&drag.off} role="button" aria-label={`${label} точка ${index+1} на графике`} aria-disabled={disabled} tabindex={disabled?-1:0} onpointerdown={event=>down(event,index)} onpointermove={update} onpointerup={finish} onpointercancel={cancel} onlostpointercapture={cancel} onkeydown={event=>key(event,index)} ondblclick={event=>{event.preventDefault();event.stopPropagation();remove(index)}}><title>{point[0].toFixed(3)} · {point[1].toFixed(3)}</title></circle>
 {/each}
</svg>
<span id={help} class="sr-only">Щелчок добавляет точку. Перетащите точку; отпускание за краем или двойной щелчок удаляет её. Стрелки — шаг 0,01, Shift — 0,1. Delete — удалить, Escape — отменить жест.</span>
<style>
 svg{width:200px;height:200px;flex-shrink:0;border:1px solid var(--border);overflow:visible;touch-action:none}.background{fill:transparent;cursor:crosshair}path{fill:none;pointer-events:none}.identity{stroke:var(--border)}.ghost{stroke-width:1;opacity:0.3}.curve{stroke-width:2}circle{cursor:move;stroke:var(--bg);stroke-width:1}circle:focus-visible,.background:focus-visible{outline:2px solid var(--accent);outline-offset:2px}.off{opacity:0.35}.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap}
</style>
