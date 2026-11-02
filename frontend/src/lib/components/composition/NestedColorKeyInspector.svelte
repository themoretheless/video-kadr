<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type ColorKey = Extract<WireVideoEffect, { kind: 'color_key' }>
  type ColorChannel='r'|'g'|'b'|'a'
  type Channel='tolerance'|'thin'|'feather'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[['tolerance','Допуск цвета',0,255,0],['thin','Сужение края',-5,5,0],['feather','Растушёвка',0,50,0]]
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: ColorKey) => ColorKey): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'color_key' ? change(effect) : effect))
  }
  function keyframe(effect: ColorKey, channel: Channel, base: number, start: number): ColorKey {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: effect.parameters[channel] ?? base }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: ColorKey, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): ColorKey {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
  function colorKey(effect:ColorKey,channel:ColorChannel,value:number,start:number):ColorKey {
    const tick=nestedComposition.timeUs-start
    if(!Number.isSafeInteger(tick)||tick<0)throw new Error('Курсор должен находиться после начала клипа')
    const previous=effect.colorAnimation?.[channel]
    if(previous?.mode==='keyframes'&&previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
    const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
    const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value}].sort((a,b)=>a.tick-b.tick)
    return {...effect,colorAnimation:{...effect.colorAnimation,[channel]:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}}
  }
  function colorEditKey(effect:ColorKey,channel:ColorChannel,tick:number,patch:{tick?:number;value?:number}|null):ColorKey {
    const previous=effect.colorAnimation?.[channel]
    if(previous?.mode!=='keyframes')return effect
    const colorAnimation={...effect.colorAnimation}
    const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
    if(keyframes.length)colorAnimation[channel]={...previous,track:{...previous.track,keyframes}}
    else delete colorAnimation[channel]
    return {...effect,colorAnimation:Object.keys(colorAnimation).length?colorAnimation:undefined}
  }
</script>
<section aria-label="Color Key">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'color_key', parameters: {} }])}>Добавить Color Key</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'color_key'}
            {@const color=effect.parameters.color ?? [0,0,1,1]}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Color Key {index + 1}</legend>
              <div class="row"><span>Цвет ключа</span>
               {#each ['R','G','B','A'] as component,componentIndex}
                {@const colorChannel=component.toLowerCase() as ColorChannel}
                {@const colorCurve=effect.colorAnimation?.[colorChannel]}
                <label>{component}<input type="number" step="0.01" aria-label={`${clip.id} Color Key color ${component}`} value={color[componentIndex]} onchange={event=>edit(track.id,clip.id,index,current=>{const next=[...(current.parameters.color ?? [0,0,1,1])] as [number,number,number,number];next[componentIndex]=Number(event.currentTarget.value);return {...current,parameters:{...current.parameters,color:next}}})} /></label>
                <button class="btn ghost sm" aria-label={`${clip.id} Color Key color ${component} key`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>colorKey(current,colorChannel,color[componentIndex],('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ {component}</button>
                {#if effect.colorAnimation?.[colorChannel]}<button class="btn ghost sm" aria-label={`${clip.id} Color Key color ${component} clear`} onclick={()=>edit(track.id,clip.id,index,current=>{const colorAnimation={...current.colorAnimation};delete colorAnimation[colorChannel];return {...current,colorAnimation:Object.keys(colorAnimation).length?colorAnimation:undefined}})}>Очистить {component}</button>{/if}
                {#if colorCurve?.mode==='keyframes'}
                 <details><summary>{component} · {colorCurve.track.keyframes.length} ключей</summary>
                  {#each colorCurve.track.keyframes as key (key.tick)}
                   <div class="row">
                    <label>Время, с<input aria-label={`${clip.id} Color Key color ${component} ${key.tick} time`} type="number" step="0.001" value={key.tick/colorCurve.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,{tick:Math.round(Number(event.currentTarget.value)*colorCurve.track.timeBase)}))} /></label>
                    <label>Значение<input aria-label={`${clip.id} Color Key color ${component} ${key.tick} value`} type="number" step="0.01" value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,{value:Number(event.currentTarget.value)}))} /></label>
                    <button class="btn ghost sm" aria-label={`${clip.id} Color Key color ${component} ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,null))}>Удалить ключ</button>
                   </div>
                  {/each}
                 </details>
                {/if}
               {/each}
              </div>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Color Key ${channel}`} type="number" {min} {max} step="0.1" value={effect.parameters[channel] ?? base} onchange={event => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, [channel]: Number(event.currentTarget.value) } }))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Color Key ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Color Key ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation={...current.animation};delete animation[channel];return {...current,animation:Object.keys(animation).length?animation:undefined} })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Color Key ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Color Key ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Color Key ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'color_key', parameters: {} }))}>Сбросить Color Key</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Color Key</button></div>
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
