<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects, editNestedMetadata } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type MetadataBurnin = Extract<WireVideoEffect, { kind: 'metadata_burnin' }>
  type Channel='positionX'|'positionY'|'size'|'colorR'|'colorG'|'colorB'|'colorA'|'opacity'
  const channels:readonly (readonly [Channel,string,number|undefined,number|undefined,number])[]=[['positionX','Позиция X',undefined,undefined,0],['positionY','Позиция Y',undefined,undefined,0],['size','Размер, %',1,100,6],['colorR','Цвет R',0,1,1],['colorG','Цвет G',0,1,1],['colorB','Цвет B',0,1,1],['colorA','Цвет A',0,1,1],['opacity','Фон, %',0,100,60]]
  function value(effect:MetadataBurnin,channel:Channel,base:number):number{
    if(channel==='positionX'||channel==='positionY'){
      const axis=channel==='positionX'?0:1
      return effect.parameters.position?.[axis]??base
    }
    if(channel.startsWith('color'))return effect.parameters.color?.[['colorR','colorG','colorB','colorA'].indexOf(channel)]??1
    return effect.parameters[channel as 'size'|'opacity']??base
  }
  function displayed(effect:MetadataBurnin,channel:Channel,base:number):number|undefined{
    if(channel==='positionX'||channel==='positionY')return effect.parameters.position?.[channel==='positionX'?0:1]??undefined
    return value(effect,channel,base)
  }
  function setValue(effect:MetadataBurnin,channel:Channel,number:number|null):MetadataBurnin{
    if(channel==='positionX'||channel==='positionY'){
      const position:[number|null,number|null]=[...(effect.parameters.position??[null,null])]
      position[channel==='positionX'?0:1]=number
      return {...effect,parameters:{...effect.parameters,position}}
    }
    if(channel.startsWith('color')){
      const color:[number,number,number,number]=[...(effect.parameters.color??[1,1,1,1])]
      color[['colorR','colorG','colorB','colorA'].indexOf(channel)]=number??1
      return {...effect,parameters:{...effect.parameters,color}}
    }
    return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}
  }
  const choices=[['source','Источник',0,['Таймкод композиции','Таймкод медиа','Имя клипа','Имя файла','Номер кадра','Имя композиции']],['alignment','Размещение',0,['Снизу по центру','Снизу слева','Снизу справа','Сверху по центру','Сверху слева','Сверху справа','Произвольно']]] as const

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  let metadata=$derived(nestedComposition.document?.compositions[compositionId]?.content.metadata)
  function edit(track: string, clip: string, index: number, change: (effect: MetadataBurnin) => MetadataBurnin): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'metadata_burnin' ? change(effect) : effect))
  }
  function keyframe(effect: MetadataBurnin, channel: Channel, base: number, start: number): MetadataBurnin {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: MetadataBurnin, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): MetadataBurnin {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Metadata Burn-in">
  <details><summary>Данные композиции</summary>
    <fieldset disabled={disabled||nestedHistory.busy}>
      <label>Имя композиции<input type="text" aria-label="Metadata Burn-in sequenceName" value={metadata?.sequenceName??''} onchange={event=>void editNestedMetadata(compositionId,current=>({...current,sequenceName:event.currentTarget.value}))} /></label>
      <label>Начальный кадр таймкода<input type="number" step="1" aria-label="Metadata Burn-in sequenceStartFrame" value={metadata?.sequenceStartFrame??0} onchange={event=>void editNestedMetadata(compositionId,current=>({...current,sequenceStartFrame:Number(event.currentTarget.value)}))} /></label>
      <label>Drop-frame<input type="checkbox" aria-label="Metadata Burn-in dropFrame" checked={metadata?.dropFrame??false} onchange={event=>void editNestedMetadata(compositionId,current=>({...current,dropFrame:event.currentTarget.checked}))} /></label>
    </fieldset>
  </details>
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'metadata_burnin', parameters: {} }])}>Добавить Metadata Burn-in</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'metadata_burnin'}
            {@const source=nestedComposition.document?.compositions[compositionId]?.content.sources?.[clip.sourceId]}
            {@const canvas=nestedComposition.document?.compositions[compositionId]?.content.canvas}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Metadata Burn-in {index + 1}</legend>
              <label>Имя клипа<input type="text" aria-label={`${clip.id} Metadata Burn-in clipName`} value={metadata?.clipNames?.[clip.id]??''} onchange={event=>void editNestedMetadata(compositionId,current=>({...current,clipNames:{...current.clipNames,[clip.id]:event.currentTarget.value}}))} /></label>
              <label>Имя файла<input type="text" aria-label={`${clip.id} Metadata Burn-in fileName`} value={metadata?.sourceNames?.[clip.sourceId]??''} onchange={event=>void editNestedMetadata(compositionId,current=>({...current,sourceNames:{...current.sourceNames,[clip.sourceId]:event.currentTarget.value}}))} /></label>
              <label>Префикс<input type="text" aria-label={`${clip.id} Metadata Burn-in prefix`} value={effect.parameters.prefix??''} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,prefix:event.currentTarget.value}}))} /></label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Metadata Burn-in ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={displayed(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Metadata Burn-in ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, channel==='positionX'?(source?.width??canvas?.width??0)*0.5:channel==='positionY'?(source?.height??canvas?.height??0)*0.5:base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Metadata Burn-in ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Metadata Burn-in ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Metadata Burn-in ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Metadata Burn-in ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} Metadata Burn-in ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'metadata_burnin', parameters: {} }))}>Сбросить Metadata Burn-in</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Metadata Burn-in</button></div>
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
