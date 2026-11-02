<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import{nestedComposition,nestedHistory,editNestedVideoEffects}from'$lib/state/nestedComposition.svelte'
 import type{WireVideoEffect}from'$lib/composition/types'
 import Wheels from'./NestedConcatColorWheelsInspector.svelte'
 import Curves from'./NestedConcatCurvesInspector.svelte'
 let{disabled=false,token=''}:{disabled?:boolean;token?:string}=$props()
 const controls=[
  ['temperature','Температура, K',2000,11000,50,6500,'Цвет'],['tint','Оттенок',-100,100,1,0,'Цвет'],['saturation','Насыщенность',-100,100,1,0,'Цвет'],
  ['exposure','Экспозиция, EV',-3,3,0.05,0,'Свет'],['brightness','Яркость',-100,100,1,0,'Свет'],['contrast','Контраст',-100,100,1,0,'Свет'],['highlights','Светлые тона',-100,100,1,0,'Свет'],['shadows','Тени',-100,100,1,0,'Свет'],['whites','Белые',-100,100,1,0,'Свет'],['blacks','Чёрные',-100,100,1,0,'Свет'],
  ['sharpen','Резкость',0,100,1,0,'Эффекты'],['fade','Выцветание',0,100,1,0,'Эффекты'],['vignette','Виньетка',0,100,1,0,'Эффекты']
 ]as const
 type Adjust=Extract<WireVideoEffect,{kind:'concat_adjust'}>
 let compositionId=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,update:(effects:readonly WireVideoEffect[])=>readonly WireVideoEffect[]){const base=nestedComposition.document,session=token,composition=compositionId;return editNestedVideoEffects(composition,track,clip,update,()=>{if(base!==nestedComposition.document||session!==token||composition!==compositionId)throw new Error('Контекст цветокоррекции изменился')})}
 function change(track:string,clip:string,index:number,update:(effect:Adjust)=>Adjust){return edit(track,clip,effects=>effects.map((effect,i)=>{if(i!==index)return effect;if(effect.kind!=='concat_adjust')throw new Error('Adjust не найден');return update(effect)}))}
</script>
<section aria-label="Concat Adjust">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void edit(track.id,clip.id,effects=>[...effects,{kind:'concat_adjust',parameters:{}}])}>Добавить Concat Adjust</button></div>
    {#each (clip.effects??[]) as effect,index (index)}
     {#if effect.kind==='concat_adjust'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Adjust {index+1}</legend>
       {#each ['Цвет','Свет','Эффекты'] as group (group)}
        <strong>{group}</strong><div class="controls">
         {#each controls.filter(control=>control[6]===group) as [channel,label,min,max,step,base] (channel)}
          <label>{label}<input type="number" {min} {max} {step} value={effect.parameters[channel]??base} aria-label={`${clip.id} Concat Adjust ${channel}`} onchange={event=>{const value=Number(event.currentTarget.value);void change(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}}/></label>
         {/each}
        </div>
       {/each}
       <div class="row"><button class="btn ghost sm" onclick={()=>void change(track.id,clip.id,index,()=>({kind:'concat_adjust',parameters:{}}))}>Сбросить Concat Adjust</button><button class="btn ghost sm" onclick={()=>void edit(track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Adjust</button></div>
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
 <Wheels kind="concat_adjust" embedded {disabled} {token}/>
 <Curves kind="concat_adjust" embedded {disabled} {token}/>
</section>
<style>
 section,fieldset{min-width:0}fieldset{border:0;padding:0.5rem 0;margin:0}.row{display:flex;gap:0.5rem;align-items:center;flex-wrap:wrap}.controls{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,15rem),1fr));gap:0.5rem;margin:0.5rem 0}label{display:flex;align-items:center;justify-content:space-between;gap:0.5rem}input{width:5rem;max-width:100%}
</style>
