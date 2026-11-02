<script lang="ts">
  import type { GraphicContent, CompositionInterpolation, CompositionExtrapolationMode } from '$lib/composition/types'
  import { graphicShapeChannels, graphicTextChannels, graphicNumericValue } from '$lib/composition/graphicLayerProperties'
  import { setGraphicSegmentCurve, replaceGraphicChannel, setGraphicExtrapolation, graphicChannelAnimation, setGraphicChannelKey, clearGraphicChannel, editGraphicChannelKey, setGraphicTextKey, editGraphicTextKey } from '$lib/composition/graphicLayerAnimation'
  import {segmentInterpolation} from '$lib/composition/tangents'
  import GraphicKeyTangents from './GraphicKeyTangents.svelte'
  import InterpolationNumbers from './InterpolationNumbers.svelte'
  let { content, label, localUs, width, height, onchange }: { content: GraphicContent; label: string; localUs: number; width: number; height: number; onchange: (content: GraphicContent) => void } = $props()
  const channels = $derived(content.kind === 'shape' ? graphicShapeChannels : graphicTextChannels)
  let channel = $state('opacity')
  const animation = $derived(graphicChannelAnimation(content,channel))
  let error = $state('')
  function commit(change: () => GraphicContent) { try { onchange(change()); error = '' } catch (reason) { error = String(reason) } }
  function key() {
    const fallback = channels.find(([key])=>key===channel)![4]
    commit(()=>setGraphicChannelKey(content,channel,localUs,graphicNumericValue(content,channel,channel==='positionX'?width/2:channel==='positionY'?height/2:fallback)))
  }
  const interpolations = ['hold','linear','ease_in','ease_out','ease_in_out','ease_in_out_cubic','bounce_in','bounce_out','bounce_in_out','bezier','elastic_in','elastic_out','elastic_in_out'] as const
  function name(value: CompositionInterpolation) { return typeof value === 'string' ? value : value.kind === 'bezier' ? 'bezier' : `elastic_${value.dir}` }
  function interpolation(value: string): CompositionInterpolation {
    if (value === 'bezier') return {kind:'bezier',p1:[.25,.1],p2:[.25,1]}
    if (value.startsWith('elastic_')) return {kind:'elastic',dir:value.slice(8) as 'in'|'out'|'in_out',amplitude:1,period:.3}
    return value as CompositionInterpolation
  }
</script>
<details>
  <summary aria-label={`${label} animation`}>Анимация слоя</summary>
  <label>Канал<select aria-label={`${label} animation channel`} value={channel} onchange={event => channel = event.currentTarget.value}>{#each channels as [key,title]}<option value={key}>{title}</option>{/each}</select></label>
  <div class="actions">
    <button class="btn ghost sm" type="button" aria-label={`${label} add key`} onclick={key}>+ Ключ у курсора</button>
    <button class="btn ghost sm" type="button" aria-label={`${label} clear channel`} disabled={!animation} onclick={()=>commit(()=>clearGraphicChannel(content,channel))}>Очистить канал</button>
  </div>
  {#if animation?.mode === 'keyframes'}
    <p>Шкала: {animation.track.timeBase} тиков/с · Время слоя: {localUs} мкс</p>
    <div class="actions">
      {#each ['before','after'] as side}
        <label>{side==='before'?'До ключей':'После ключей'}<select aria-label={`${label} extrapolation ${side}`} value={animation.track.extrapolation?.[side as 'before'|'after']??'hold'} onchange={event=>commit(()=>setGraphicExtrapolation(content,channel,side as 'before'|'after',event.currentTarget.value as CompositionExtrapolationMode))}>
          {#each ['hold','continue','loop','ping_pong','offset'] as mode}<option value={mode}>{mode}</option>{/each}
        </select></label>
      {/each}
    </div>
    {#each animation.track.keyframes as key, index}
      <div class="key">
        <label>Тик<input aria-label={`${label} ${channel} key ${index} tick`} type="number" step="1" value={key.tick} onchange={event=>commit(()=>editGraphicChannelKey(content,channel,index,{tick:Number(event.currentTarget.value)}))} /></label>
        <label>Значение<input aria-label={`${label} ${channel} key ${index} value`} type="number" step="any" value={key.value} onchange={event=>commit(()=>editGraphicChannelKey(content,channel,index,{value:Number(event.currentTarget.value)}))} /></label>
        <label>Переход<select aria-label={`${label} ${channel} key ${index} interpolation`} value={name(key.interpolation??animation.track.interpolation)} onchange={event=>commit(()=>editGraphicChannelKey(content,channel,index,{interpolation:interpolation(event.currentTarget.value)}))}>
          {#each interpolations as mode}<option value={mode}>{mode}</option>{/each}
        </select></label>
        <InterpolationNumbers value={segmentInterpolation(key,animation.track.keyframes[index+1]??key,animation.track.interpolation)} label={`${label} ${channel} key ${index}`} onChange={value=>commit(()=>setGraphicSegmentCurve(content,channel,index,value))} />
        <GraphicKeyTangents {animation} tick={key.tick} label={`${label} ${channel} key ${index}`} onchange={value=>commit(()=>replaceGraphicChannel(content,channel,value))} />
        <button class="btn ghost sm" type="button" aria-label={`${label} ${channel} key ${index} delete`} onclick={()=>commit(()=>editGraphicChannelKey(content,channel,index,null))}>Удалить ключ</button>
      </div>
    {/each}
  {/if}
  {#if content.kind === 'text'}
    <button class="btn ghost sm" type="button" aria-label={`${label} add text key`} onclick={()=>commit(()=>setGraphicTextKey(content,localUs,content.kind==='text'?content.parameters.text??'Текст':''))}>+ Ключ исходного текста</button>
    {#if content.sourceTextAnimation}<p>Текст: {content.sourceTextAnimation.timeBase} тиков/с · Hold</p>{/if}
    {#each content.sourceTextAnimation?.keyframes ?? [] as key, index}
      <div class="key">
        <label>Тик<input aria-label={`${label} text key ${index} tick`} type="number" step="1" value={key.tick} onchange={event=>commit(()=>editGraphicTextKey(content,index,{tick:Number(event.currentTarget.value)}))} /></label>
        <label>Текст<textarea aria-label={`${label} text key ${index} value`} value={key.value} onchange={event=>commit(()=>editGraphicTextKey(content,index,{value:event.currentTarget.value}))}></textarea></label>
        <button class="btn ghost sm" type="button" aria-label={`${label} text key ${index} delete`} onclick={()=>commit(()=>editGraphicTextKey(content,index,null))}>Удалить текстовый ключ</button>
      </div>
    {/each}
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>summary {cursor:pointer;margin:.4rem 0}.actions {display:flex;gap:.3rem;flex-wrap:wrap}.key {display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:.3rem;padding:.4rem 0;border-bottom:1px solid var(--border,#444)}label {display:flex;flex-direction:column;min-width:0;font-size:.8rem}input,select,textarea {min-width:0;max-width:100%}p {font-size:.75rem}</style>
