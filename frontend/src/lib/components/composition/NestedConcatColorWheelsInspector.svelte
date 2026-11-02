<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks,concatEffectNativeKeyOrigin} from '$lib/composition/concatEffectInspectorContext'
 import ConcatColorWheelsSourceKeyEditor from "./ConcatColorWheelsSourceKeyEditor.svelte"
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import {sampleWheel,setWheel,keyWheel,wheelPreviewProject} from '$lib/composition/concatWheelAuthoring'
 import {nestedEffectPreview,clearEffectPreview}from'$lib/state/nestedEffectPreview.svelte'
 import ColorWheelPuck,{type ColorWheelDraft} from './ColorWheelPuck.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false,token='',kind='concat_color_wheels',embedded=false}:{disabled?:boolean;token?:string;kind?:'concat_color_wheels'|'concat_adjust';embedded?:boolean}=$props()
 const title=$derived(kind==='concat_adjust'?'Concat Adjust':'Concat Color Wheels')
 const previewOwner=Symbol('color-wheel-preview')
 let pendingGesture:object|undefined
 type Wheel='lift'|'gamma'|'gain'
 type SharpChannel=`${Wheel}.${'x'|'y'|'m'}`
 type Hsl=Extract<WireVideoEffect,{kind:'concat_color_wheels'|'concat_adjust'}>
 function hasSource(effect:Hsl):boolean{return Object.values(effect.concatKeys??{}).some(run=>run.length>0)}
 function matches(effect:WireVideoEffect):effect is Hsl{return effect.kind===kind}
 const channels:readonly (readonly [SharpChannel,string,number,number,number,number])[]=(['lift','gamma','gain']as const).flatMap(wheel=>(['x','y','m']as const).map(axis=>[`${wheel}.${axis}`as SharpChannel,`${wheel} · ${axis==='m'?'Master':axis.toUpperCase()}`,-1,1,0.01,0]as const))
 function value(effect:Hsl,channel:SharpChannel):number {const [wheel,axis]=channel.split('.')as[Wheel,string];return effect.parameters[wheel]?.[axis==='x'?0:axis==='y'?1:2]??0}
 function changed(effect:Hsl,channel:SharpChannel,v:number):Hsl {const [wheel,axis]=channel.split('.')as[Wheel,string];const coordinates:[number,number,number]=[...(effect.parameters[wheel]??[0,0,0])];coordinates[axis==='x'?0:axis==='y'?1:2]=v;return{...effect,parameters:{...effect.parameters,[wheel]:coordinates}}}
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let previewContext=$derived(`${compositionId}:${kind}`)
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function editEffects(track:string,clip:string,update:(effects:readonly WireVideoEffect[])=>readonly WireVideoEffect[]):Promise<void>{
  const base=nestedComposition.document,session=token,composition=compositionId,mode=kind
  return editNestedVideoEffects(composition,track,clip,update,()=>{if(base!==nestedComposition.document||token!==session||compositionId!==composition||kind!==mode)throw new Error('Контекст цветокоррекции изменился')})
 }
 function edit(track:string,clip:string,index:number,change:(effect:Hsl)=>Hsl):Promise<void> {return editEffects(track,clip,effects=>effects.map((effect,i)=>i===index&&matches(effect)?change(effect):effect))}
 function wheelFromChannel(channel:SharpChannel):Wheel{return channel.split('.')[0] as Wheel}
 function clearWheelKeys(current:Hsl,channel:SharpChannel):Hsl{const animation={...current.animation};for(const axis of ['x','y','m'])delete animation[`${wheelFromChannel(channel)}.${axis}` as SharpChannel];return {...current,animation:Object.keys(animation).length?animation:undefined}}
 function wheelKey(effect:Hsl,wheel:Wheel,start:number):Hsl {return keyWheel(effect,wheel,nestedComposition.timeUs-start,effect.parameters[wheel]??[0,0,0])}
 async function puck(track:string,clip:string,index:number,wheel:Wheel,x:number,y:number,scope:unknown,start:number){if(nestedComposition.document!==scope)return;const time=nestedComposition.timeUs-start;const active=nestedEffectPreview.value;const gesture=active?.owner===previewOwner?active.gesture:undefined;pendingGesture=gesture;try{await edit(track,clip,index,current=>setWheel(current,wheel,{x,y},time))}finally{clearEffectPreview(previewOwner,gesture);pendingGesture=undefined}}
 function preview(track:string,clip:string,index:number,wheel:Wheel,start:number,draft:ColorWheelDraft|null,gesture:object){
  if(!draft){if(pendingGesture!==gesture)clearEffectPreview(previewOwner,gesture);return}
  const base=nestedComposition.document;if(!base||base!==draft.scope||nestedComposition.timeUs!==draft.clock||token!==draft.session||previewContext!==draft.context)return
  try{const project=wheelPreviewProject(base,compositionId,track,clip,index,wheel,{x:draft.x,y:draft.y},draft.clock-start,kind);nestedEffectPreview.value={owner:previewOwner,gesture,base,project,timeUs:draft.clock,compositionId,token}}catch{clearEffectPreview(previewOwner,gesture)}
 }
 $effect(()=>{const d=nestedEffectPreview.value;if(d?.owner===previewOwner&&(disabled||d.base!==nestedComposition.document||d.timeUs!==nestedComposition.timeUs||d.compositionId!==compositionId||d.token!==token))clearEffectPreview(previewOwner)})
 $effect(()=>()=>clearEffectPreview(previewOwner))
 function master(track:string,clip:string,index:number,wheel:Wheel,value:number,start:number){const time=nestedComposition.timeUs-start;edit(track,clip,index,current=>setWheel(current,wheel,{m:value},time))}

</script>
<section aria-label={title}>
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    {#if !embedded}<div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editEffects(track.id,clip.id,effects=>[...effects,{kind,parameters:{}}])}>Добавить {title}</button></div>{/if}
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if matches(effect)}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>{title} {index+1} · колёса</legend>

       {#if hasSource(effect)}<p>Кадром управляют исходные ключи Concat. Для редактирования колёс сначала удалите их.</p>{/if}
       <fieldset disabled={hasSource(effect)}>
       <div class="wheels">
        {#each (['lift','gamma','gain']as const) as wheel (wheel)}
         {@const start=concatEffectNativeKeyOrigin(clip)}
          {@const sampled=sampleWheel(effect,wheel,nestedComposition.timeUs-start)}
         <div class="wheel">
          <strong>{wheel} · текущий кадр</strong>
          <ColorWheelPuck x={sampled[0]} y={sampled[1]} clock={nestedComposition.timeUs} session={token} context={previewContext} onpreview={(draft,gesture)=>preview(track.id,clip.id,index,wheel,start,draft,gesture)} label={`${clip.id} ${title} ${wheel} цветовое колесо`} disabled={hasSource(effect)||disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)||nestedComposition.timeUs<start} scope={nestedComposition.document} oncommit={(x,y,scope)=>puck(track.id,clip.id,index,wheel,x,y,scope,start)} />
          <label>Master <input type="range" min="-1" max="1" step="0.01" aria-label={`${clip.id} ${title} ${wheel} Master`} disabled={hasSource(effect)||nestedComposition.timeUs<start} value={sampled[2]} onchange={event=>{const v=Number(event.currentTarget.value);master(track.id,clip.id,index,wheel,v,start)}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} ${title} ${wheel} общий ключ`} disabled={hasSource(effect)||nestedComposition.timeUs<(concatEffectNativeKeyOrigin(clip))} onclick={()=>edit(track.id,clip.id,index,current=>keyWheel(current,wheel,nestedComposition.timeUs-start))}>Ключ {wheel}</button>
         </div>
        {/each}
       </div>
       <div class="channels">
        {#each channels as [channel,label,min,max,step] (channel)}
         {@const animation=effect.animation?.[channel]}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" disabled={hasSource(effect)} {min} {max} {step} aria-label={`${clip.id} ${title} ${channel}`} value={value(effect,channel)} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>changed(current,channel,value))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} ${title} ${channel} ключ`} disabled={hasSource(effect)||nestedComposition.timeUs<(concatEffectNativeKeyOrigin(clip))} onclick={()=>edit(track.id,clip.id,index,current=>wheelKey(current,wheelFromChannel(channel),concatEffectNativeKeyOrigin(clip)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} ${title} ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>clearWheelKeys(current,channel))}>Удалить анимацию</button>{/if}</div>
          {#if animation?.mode==='keyframes'}<small>{animation.track.keyframes.map(p=>`${p.tick}: ${p.value}`).join(' · ')}</small>{/if}
         </div>
        {/each}
       </div>
       </fieldset>
       {#if matches(effect)}
        {#each channels as [channel] (channel)}
         <ConcatColorWheelsSourceKeyEditor {effect} {channel} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)||!!effect.animation} onchange={change=>edit(track.id,clip.id,index,current=>change(current))} />
        {/each}
       {/if}
       {#if hasSource(effect)}<button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       {#if !embedded}
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind,parameters:{}}))}>Сбросить {title}</button>
       <button class="btn ghost sm" onclick={()=>void editEffects(track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить {title}</button>
       {/if}
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
</section>
<style>
 .wheels{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,9rem),1fr));gap:0.75rem;max-width:32rem;}.wheel{display:flex;flex-direction:column;align-items:center;gap:0.4rem;min-width:0;}.wheel input{max-width:100%;}
 .row {display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap;}
 section{min-width:0;}
 fieldset {min-width:0;box-sizing:border-box;width:100%;border:0;padding:0.5rem 0;margin:0;}
 .channels {display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,22rem),1fr));gap:0.25rem 0.75rem;max-width:60rem;}
 input[type=number] {width:4.5rem;} small {display:block;}
</style>
