<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type UltraKey = Extract<WireVideoEffect, { kind: 'ultra_key' }>
  type ColorChannel='r'|'g'|'b'|'a'
  type Channel='transparency'|'highlight'|'shadow'|'tolerance'|'pedestal'|'choke'|'soften'|'spill'|'contrast'|'midPoint'|'desaturate'|'range'|'spillLuma'|'ccSaturation'|'ccHue'|'ccLuminance'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[['transparency','Прозрачность',0,100,45],['highlight','Света',0,100,10],['shadow','Тени',0,100,50],['tolerance','Допуск',0,100,50],['pedestal','Пьедестал',0,100,10],['choke','Сужение',0,100,0],['soften','Смягчение',0,100,0],['spill','Подавление spill',0,100,50],['contrast','Контраст матте',0,100,0],['midPoint','Средняя точка',0,100,50],['desaturate','Обесцвечивание spill',0,100,25],['range','Диапазон spill',0,100,50],['spillLuma','Яркость spill',0,100,50],['ccSaturation','Насыщенность',0,200,100],['ccHue','Оттенок',-3.4028234663852886e38,3.4028234663852886e38,0],['ccLuminance','Светлота',0,200,100]]
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: UltraKey) => UltraKey): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'ultra_key' ? change(effect) : effect))
  }
  function keyframe(effect: UltraKey, channel: Channel, base: number, start: number): UltraKey {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: effect.parameters[channel] ?? base }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: UltraKey, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): UltraKey {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
  function colorKey(effect:UltraKey,channel:ColorChannel,value:number,start:number):UltraKey {
    const tick=nestedComposition.timeUs-start
    if(!Number.isSafeInteger(tick)||tick<0)throw new Error('Курсор должен находиться после начала клипа')
    const previous=effect.colorAnimation?.[channel]
    if(previous?.mode==='keyframes'&&previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
    const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
    const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value}].sort((a,b)=>a.tick-b.tick)
    return {...effect,colorAnimation:{...effect.colorAnimation,[channel]:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}}
  }
  function colorEditKey(effect:UltraKey,channel:ColorChannel,tick:number,patch:{tick?:number;value?:number}|null):UltraKey {
    const previous=effect.colorAnimation?.[channel]
    if(previous?.mode!=='keyframes')return effect
    const colorAnimation={...effect.colorAnimation}
    const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
    if(keyframes.length)colorAnimation[channel]={...previous,track:{...previous.track,keyframes}}
    else delete colorAnimation[channel]
    return {...effect,colorAnimation:Object.keys(colorAnimation).length?colorAnimation:undefined}
  }
</script>
<section aria-label="Ultra Key">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'ultra_key', parameters: {} }])}>Добавить Ultra Key</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'ultra_key'}
            {@const color=effect.parameters.keyColor ?? [0,0.8,0.2,1]}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Ultra Key {index + 1}</legend>
              <label>Вывод<select aria-label={`${clip.id} Ultra Key output`} value={effect.parameters.output ?? 0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,output:Number(event.currentTarget.value) as 0|1|2}}))}><option value="0">Composite</option><option value="1">Alpha Channel</option><option value="2">Color Channel</option></select></label>
              <label>Настройка<select aria-label={`${clip.id} Ultra Key setting`} value={effect.parameters.setting ?? 0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,setting:Number(event.currentTarget.value) as 0|1|2|3}}))}><option value="0">Default</option><option value="1">Relaxed</option><option value="2">Aggressive</option><option value="3">Custom</option></select></label>

              <div class="row"><span>Цвет ключа</span>
               {#each ['R','G','B','A'] as component,componentIndex}
                {@const colorChannel=component.toLowerCase() as ColorChannel}
                {@const colorCurve=effect.colorAnimation?.[colorChannel]}
                <label>{component}<input type="number" step="0.01" aria-label={`${clip.id} Ultra Key color ${component}`} value={color[componentIndex]} onchange={event=>edit(track.id,clip.id,index,current=>{const next=[...(current.parameters.keyColor ?? [0,0.8,0.2,1])] as [number,number,number,number];next[componentIndex]=Number(event.currentTarget.value);return {...current,parameters:{...current.parameters,keyColor:next}}})} /></label>
                <button class="btn ghost sm" aria-label={`${clip.id} Ultra Key color ${component} key`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>colorKey(current,colorChannel,color[componentIndex],('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ {component}</button>
                {#if effect.colorAnimation?.[colorChannel]}<button class="btn ghost sm" aria-label={`${clip.id} Ultra Key color ${component} clear`} onclick={()=>edit(track.id,clip.id,index,current=>{const colorAnimation={...current.colorAnimation};delete colorAnimation[colorChannel];return {...current,colorAnimation:Object.keys(colorAnimation).length?colorAnimation:undefined}})}>Очистить {component}</button>{/if}
                {#if colorCurve?.mode==='keyframes'}
                 <details><summary>{component} · {colorCurve.track.keyframes.length} ключей</summary>
                  {#each colorCurve.track.keyframes as key (key.tick)}
                   <div class="row">
                    <label>Время, с<input aria-label={`${clip.id} Ultra Key color ${component} ${key.tick} time`} type="number" step="0.001" value={key.tick/colorCurve.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,{tick:Math.round(Number(event.currentTarget.value)*colorCurve.track.timeBase)}))} /></label>
                    <label>Значение<input aria-label={`${clip.id} Ultra Key color ${component} ${key.tick} value`} type="number" step="0.01" value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,{value:Number(event.currentTarget.value)}))} /></label>
                    <button class="btn ghost sm" aria-label={`${clip.id} Ultra Key color ${component} ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,null))}>Удалить ключ</button>
                   </div>
                  {/each}
                 </details>
                {/if}
               {/each}
              </div>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Ultra Key ${channel}`} type="number" {min} {max} step="0.1" value={effect.parameters[channel] ?? base} onchange={event => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, [channel]: Number(event.currentTarget.value) } }))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Ultra Key ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Ultra Key ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation={...current.animation};delete animation[channel];return {...current,animation:Object.keys(animation).length?animation:undefined} })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Ultra Key ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Ultra Key ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Ultra Key ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'ultra_key', parameters: {} }))}>Сбросить Ultra Key</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Ultra Key</button></div>
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
