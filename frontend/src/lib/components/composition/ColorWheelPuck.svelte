<script module lang="ts">
 export interface ColorWheelDraft {x:number;y:number;scope:unknown;clock:number;session:string;context:string;gesture:object}
</script>
<script lang="ts">
 import {untrack}from'svelte'
 let {x=0,y=0,label,disabled=false,scope,clock=0,session='',context='',oncommit,onpreview}:{x?:number;y?:number;label:string;disabled?:boolean;scope:unknown;clock?:number;session?:string;context?:string;onpreview?:(draft:ColorWheelDraft|null,gesture:object)=>void;oncommit:(x:number,y:number,scope:unknown)=>void}=$props()
 type Drag={id:number;x:number;y:number;scope:unknown;clock:number;session:string;context:string;gesture:object}
 const helpId=$props.id()
 let drag:Drag|undefined=$state.raw()
 function disc(x:number,y:number):[number,number]{const radius=Math.hypot(x,y);return radius>1?[x/radius,y/radius]:[x,y]}
 let point=$derived(drag?[drag.x,drag.y]:disc(x,y))
 $effect(()=>{const current=scope;const time=clock;const identity=session;const place=context;if(drag&&(disabled||drag.scope!==current||drag.clock!==time||drag.session!==identity||drag.context!==place))untrack(drop)})
 function at(event:PointerEvent):[number,number]{const rect=event.currentTarget instanceof HTMLElement?event.currentTarget.getBoundingClientRect():undefined;if(!rect||rect.width<=0||rect.height<=0)return disc(x,y);return disc((event.clientX-rect.left-rect.width/2)/(rect.width/2),-(event.clientY-rect.top-rect.height/2)/(rect.height/2))}
 function down(event:PointerEvent){if(disabled||event.button!==0||drag)return;event.preventDefault();const [px,py]=at(event);drag={id:event.pointerId,x:px,y:py,scope,clock,session,context,gesture:{}};(event.currentTarget as HTMLElement).focus({preventScroll:true});(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);onpreview?.(drag,drag.gesture)}
 function move(event:PointerEvent){if(!drag||drag.id!==event.pointerId)return;const [px,py]=at(event);drag={...drag,x:px,y:py};onpreview?.(drag,drag.gesture)}
 function finish(event:PointerEvent){if(!drag||drag.id!==event.pointerId)return;const previous=drag;const [px,py]=at(event);drag=undefined;if(!disabled&&previous.scope===scope&&previous.clock===clock&&previous.session===session&&previous.context===context&&(px!==x||py!==y))oncommit(px,py,previous.scope);onpreview?.(null,previous.gesture)}
 function drop(){if(!drag)return;const previous=drag;drag=undefined;onpreview?.(null,previous.gesture)}
 $effect(()=>()=>drop())
 function cancel(event:PointerEvent){if(drag?.id===event.pointerId)drop()}
 function keyboard(event:KeyboardEvent){if(event.key==='Escape'&&drag){event.preventDefault();drop();return}if(drag)return;if(disabled||event.ctrlKey||event.metaKey||event.altKey)return;let [px,py]=disc(x,y);const step=event.shiftKey?0.1:0.01;switch(event.key){case'ArrowLeft':px-=step;break;case'ArrowRight':px+=step;break;case'ArrowUp':py+=step;break;case'ArrowDown':py-=step;break;case'Home':px=0;py=0;break;default:return}event.preventDefault();[px,py]=disc(px,py);if(px!==x||py!==y)oncommit(px,py,scope)}
</script>
<button type="button" class="disc" {disabled} aria-label={label} aria-describedby={helpId} title={`X ${point[0]!.toFixed(3)} · Y ${point[1]!.toFixed(3)}`} onpointerdown={down} onpointermove={move} onpointerup={finish} onpointercancel={cancel} onlostpointercapture={cancel} onkeydown={keyboard}>
 <span class="puck" style:left={`${50+point[0]!*50}%`} style:top={`${50-point[1]!*50}%`}></span>
</button>
<span id={helpId} class="sr-only">Перетащите точку. Стрелки — шаг 0,01; Shift — 0,1; Home — центр.</span>
<style>
 .sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap;}
 .disc{position:relative;display:block;width:min(100%,8rem);aspect-ratio:1;border-radius:50%;border:1px solid var(--border);padding:0;touch-action:none;background:radial-gradient(circle,white 0%,transparent 72%),conic-gradient(red,yellow,lime,cyan,blue,magenta,red);cursor:crosshair;}
 .disc:disabled{opacity:0.5;cursor:default;}.disc:focus-visible{outline:2px solid var(--accent);outline-offset:3px;}
 .puck{position:absolute;width:0.65rem;height:0.65rem;border:2px solid white;box-shadow:0 0 0 1px black;border-radius:50%;transform:translate(-50%,-50%);pointer-events:none;}
</style>
