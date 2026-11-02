<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type Lightning = Extract<WireVideoEffect, { kind: 'lightning' }>
  type Channel='startX'|'startY'|'endX'|'endY'|'segments'|'amplitude'|'detail'|'detailAmplitude'|'branching'|'speed'|'width'|'core'|'seed'|'outsideR'|'outsideG'|'outsideB'|'outsideA'|'insideR'|'insideG'|'insideB'|'insideA'
  const channels:readonly (readonly [Channel,string,number|undefined,number|undefined,number])[]=[['startX','Начало X',undefined,undefined,0],['startY','Начало Y',undefined,undefined,0],['endX','Конец X',undefined,undefined,0],['endY','Конец Y',undefined,undefined,0],['segments','Сегменты',1,100,7],['amplitude','Амплитуда, %',0,100,10],['detail','Детализация',0,10,2],['detailAmplitude','Амплитуда деталей',0,1,0.3],['branching','Ветвление',0,1,0.3],['speed','Скорость',0,100,10],['width','Толщина',0,200,10],['core','Ядро',0,1,0.4],['seed','Seed',0,10000,0],['outsideR','Свечение R',0,1,0.35],['outsideG','Свечение G',0,1,0.3],['outsideB','Свечение B',0,1,1],['outsideA','Свечение A',0,1,1],['insideR','Ядро R',0,1,1],['insideG','Ядро G',0,1,1],['insideB','Ядро B',0,1,1],['insideA','Ядро A',0,1,1]]

 function value(effect:Lightning,channel:Channel,base:number):number{
  if(channel==='startX'||channel==='startY')return effect.parameters.start?.[channel==='startX'?0:1]??base
  if(channel==='endX'||channel==='endY')return effect.parameters.end?.[channel==='endX'?0:1]??base
  if(channel.startsWith('outside'))return effect.parameters.outside?.[['outsideR','outsideG','outsideB','outsideA'].indexOf(channel)]??base
  if(channel.startsWith('inside'))return effect.parameters.inside?.[['insideR','insideG','insideB','insideA'].indexOf(channel)]??base
  return effect.parameters[channel as 'segments'|'amplitude'|'detail'|'detailAmplitude'|'branching'|'speed'|'width'|'core'|'seed']??base
 }
 function displayed(effect:Lightning,channel:Channel,base:number):number|undefined{
  if(channel==='startX'||channel==='startY')return effect.parameters.start?.[channel==='startX'?0:1]??undefined
  if(channel==='endX'||channel==='endY')return effect.parameters.end?.[channel==='endX'?0:1]??undefined
  return value(effect,channel,base)
 }
 function setValue(effect:Lightning,channel:Channel,number:number|null):Lightning{
  if(channel==='startX'||channel==='startY'||channel==='endX'||channel==='endY'){
   const group=channel.startsWith('start')?'start':'end';const point:[number|null,number|null]=[...(effect.parameters[group]??[null,null])];point[channel.endsWith('X')?0:1]=number;return {...effect,parameters:{...effect.parameters,[group]:point}}
  }
  if(channel.startsWith('outside')||channel.startsWith('inside')){
   const group=channel.startsWith('outside')?'outside':'inside';const defaults:[number,number,number,number]=group==='outside'?[.35,.3,1,1]:[1,1,1,1];const color:[number,number,number,number]=[...(effect.parameters[group]??defaults)];color['RGBA'.indexOf(channel.slice(-1))]=number??channels.find(([name])=>name===channel)![4];return {...effect,parameters:{...effect.parameters,[group]:color}}
  }
  return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}
 }
 const choices=[['mode','Смешивание',1,['Normal','Add','Screen','Multiply','Overlay']]]as const
 function automatic(channel:Channel,base:number,w:number,h:number):number{return channel==='startX'?w*.1:channel==='startY'?h*.1:channel==='endX'?w*.9:channel==='endY'?h*.9:base}

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: Lightning) => Lightning): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'lightning' ? change(effect) : effect))
  }
  function keyframe(effect: Lightning, channel: Channel, base: number, start: number): Lightning {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: Lightning, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): Lightning {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Lightning">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'lightning', parameters: {} }])}>Добавить Lightning</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'lightning'}
            {@const source=nestedComposition.document?.compositions[compositionId]?.content.sources?.[clip.sourceId]}
            {@const canvas=nestedComposition.document?.compositions[compositionId]?.content.canvas}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Lightning {index + 1}</legend>

              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Lightning ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={displayed(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Lightning ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, automatic(channel,base,source?.width??canvas?.width??0,source?.height??canvas?.height??0), ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Lightning ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Lightning ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Lightning ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Lightning ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} Lightning ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'lightning', parameters: {} }))}>Сбросить Lightning</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Lightning</button></div>
            </fieldset>
          {/if}
        {/each}
      {/each}
    {/if}
  {/each}
</section>
<style>
  section, fieldset { display: grid; gap: 6px; }
  fieldset { border: 0; padding: 0; margin: 0; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  label { display: flex; gap: 6px; align-items: center; }
  input[type=number] { width: 5.5rem; }
</style>
