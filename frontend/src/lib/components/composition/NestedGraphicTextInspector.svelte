<script lang="ts">
  import GraphicCharacterFormatting from './GraphicCharacterFormatting.svelte'
  import {editGraphicTextRuns} from '$lib/api'

  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type GraphicText = Extract<WireVideoEffect, { kind: 'graphic_text' }>
  type Channel='size'|'tracking'|'leading'|'baselineShift'|'boxWidth'|'boxHeight'|'positionX'|'positionY'|'anchorX'|'anchorY'|'fillR'|'fillG'|'fillB'|'fillA'|'strokeR'|'strokeG'|'strokeB'|'strokeA'|'stroke2R'|'stroke2G'|'stroke2B'|'stroke2A'|'backgroundR'|'backgroundG'|'backgroundB'|'backgroundA'|'shadowR'|'shadowG'|'shadowB'|'shadowA'|'strokeWidth'|'stroke2Width'|'backgroundOpacity'|'backgroundSize'|'backgroundRadius'|'shadowOpacity'|'shadowAngle'|'shadowDistance'|'shadowSize'|'shadowBlur'|'scale'|'scaleWidth'|'rotation'|'opacity'
  const channels:readonly (readonly [Channel,string,number|undefined,number|undefined,number])[]=[["size", "Размер", 1, 2000, 100], ["tracking", "Трекинг", -1000, 10000, 0], ["leading", "Интерлиньяж", -5000, 5000, 0], ["baselineShift", "Смещение базовой линии", -5000, 5000, 0], ["boxWidth", "Ширина текста", 0, 100000, 0], ["boxHeight", "Высота текста", 0, 100000, 0], ["positionX", "position X", undefined, undefined, 0], ["positionY", "position Y", undefined, undefined, 0], ["anchorX", "anchor X", undefined, undefined, 0], ["anchorY", "anchor Y", undefined, undefined, 0], ["fillR", "fill R", 0, 1, 1], ["fillG", "fill G", 0, 1, 1], ["fillB", "fill B", 0, 1, 1], ["fillA", "fill A", 0, 1, 1], ["strokeR", "stroke R", 0, 1, 0], ["strokeG", "stroke G", 0, 1, 0], ["strokeB", "stroke B", 0, 1, 0], ["strokeA", "stroke A", 0, 1, 1], ["stroke2R", "stroke2 R", 0, 1, 1], ["stroke2G", "stroke2 G", 0, 1, 0.85], ["stroke2B", "stroke2 B", 0, 1, 0.2], ["stroke2A", "stroke2 A", 0, 1, 1], ["backgroundR", "background R", 0, 1, 0], ["backgroundG", "background G", 0, 1, 0], ["backgroundB", "background B", 0, 1, 0], ["backgroundA", "background A", 0, 1, 1], ["shadowR", "shadow R", 0, 1, 0], ["shadowG", "shadow G", 0, 1, 0], ["shadowB", "shadow B", 0, 1, 0], ["shadowA", "shadow A", 0, 1, 1], ["strokeWidth", "strokeWidth", 0, 1000, 4], ["stroke2Width", "stroke2Width", 0, 1000, 10], ["backgroundOpacity", "backgroundOpacity", 0, 100, 75], ["backgroundSize", "backgroundSize", 0, 1000, 12], ["backgroundRadius", "backgroundRadius", 0, 1000, 0], ["shadowOpacity", "shadowOpacity", 0, 100, 75], ["shadowAngle", "shadowAngle", undefined, undefined, 135], ["shadowDistance", "shadowDistance", 0, 1000, 10], ["shadowSize", "shadowSize", 0, 1000, 0], ["shadowBlur", "shadowBlur", 0, 1000, 40], ["scale", "scale", 0, 100000, 100], ["scaleWidth", "scaleWidth", 0, 100000, 100], ["rotation", "rotation", undefined, undefined, 0], ["opacity", "opacity", 0, 100, 100]]
 const colorDefaults={"fill": [1, 1, 1, 1], "stroke": [0, 0, 0, 1], "stroke2": [1, 0.85, 0.2, 1], "background": [0, 0, 0, 1], "shadow": [0, 0, 0, 1]} as const
 const flags=[['kerning','Кернинг',true],['ligatures','Лигатуры',true],['fauxBold','Полужирный',false],['fauxItalic','Курсив',false],['underline','Подчёркивание',false],['vertical','Вертикальный текст',false],['fill','Заливка',true],['stroke','Обводка',false],['stroke2','Вторая обводка',false],['background','Фон',false],['shadow','Тень',false],['uniformScale','Сохранять пропорции',true]] as const
 const choices=[['align','Выравнивание',0,['Слева','По центру','Справа','По ширине']],['caps','Регистр',0,['Обычный','Прописные','Капитель']],['strokeType','Первая обводка',0,['Снаружи','По центру','Внутри']],['stroke2Type','Вторая обводка',0,['Снаружи','По центру','Внутри']]] as const
 function value(effect:GraphicText,channel:Channel,base:number):number {
  for(const group of ['position','anchor']as const)if(channel===group+'X'||channel===group+'Y')return effect.parameters[group]?.[channel.endsWith('X')?0:1]??base
  for(const group of ['fill','stroke','stroke2','background','shadow']as const)if(['R','G','B','A'].some(axis=>channel===group+axis))return effect.parameters[`${group}Color`]?.['RGBA'.indexOf(channel.slice(-1))]??base
  return effect.parameters[channel as 'size'|'tracking'|'leading'|'baselineShift'|'boxWidth'|'boxHeight'|'strokeWidth'|'stroke2Width'|'backgroundOpacity'|'backgroundSize'|'backgroundRadius'|'shadowOpacity'|'shadowAngle'|'shadowDistance'|'shadowSize'|'shadowBlur'|'scale'|'scaleWidth'|'rotation'|'opacity']??base
 }
 function displayed(effect:GraphicText,channel:Channel,base:number):number|undefined{return channel.startsWith('position')&&effect.parameters.position==null?undefined:value(effect,channel,base)}
 function automatic(channel:Channel,base:number,w:number,h:number):number{return channel==='positionX'?w/2:channel==='positionY'?h/2:base}
 function setValue(effect:GraphicText,channel:Channel,number:number|null,w:number,h:number):GraphicText {
  for(const group of ['position','anchor']as const)if(channel===group+'X'||channel===group+'Y'){
   if(group==='position'&&number==null)return {...effect,parameters:{...effect.parameters,position:null}}
   const defaults:readonly[number,number]=group==='position'?[w/2,h/2]:[0,0]
   const point:[number,number]=[...(effect.parameters[group]??defaults)];point[channel.endsWith('X')?0:1]=number??defaults[channel.endsWith('X')?0:1];return {...effect,parameters:{...effect.parameters,[group]:point}}
  }
  for(const group of ['fill','stroke','stroke2','background','shadow']as const)if(['R','G','B','A'].some(axis=>channel===group+axis)){
   const color:[number,number,number,number]=[...(effect.parameters[`${group}Color`]??colorDefaults[group])];const i='RGBA'.indexOf(channel.slice(-1));color[i]=number??colorDefaults[group][i]!;return {...effect,parameters:{...effect.parameters,[`${group}Color`]:color}}
  }
  return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}
 }
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: GraphicText) => GraphicText): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'graphic_text' ? change(effect) : effect))
  }
  let textBusy=$state(false)
  let textError=$state('')
  async function textField(track:string,clip:string,index:number,field:string,value:string,original:GraphicText):Promise<void> {
    if(field!=='text'||!original.parameters.runs?.length){edit(track,clip,index,current=>({...current,parameters:{...current.parameters,[field]:value}}));return}
    textBusy=true;textError=''
    try {
      const runs=await editGraphicTextRuns(original.parameters.text??'',original.parameters.runs,{kind:'adjust',newText:value})
      await editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((current,i)=>{
        if(i!==index)return current
        if(current.kind!=='graphic_text'||JSON.stringify(current.parameters)!==JSON.stringify(original.parameters))throw new Error('Текст изменился во время редактирования; повторите действие')
        return {...current,parameters:{...current.parameters,text:value,runs}}
      }))
    }catch(error){textError=String(error)}finally{textBusy=false}
  }
  function keyframe(effect: GraphicText, channel: Channel, base: number, start: number): GraphicText {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function sourceKey(effect:GraphicText,start:number):GraphicText {
    const tick=nestedComposition.timeUs-start
    if(!Number.isSafeInteger(tick)||tick<0)throw new Error('Курсор должен находиться после начала клипа')
    const previous=effect.sourceTextAnimation
    if(previous&&previous.timeBase!==1000000)throw new Error('Шкала текста должна быть в микросекундах')
    const keyframes=[...(previous?.keyframes??[]).filter(key=>key.tick!==tick),{tick,value:effect.parameters.text??''}].sort((a,b)=>a.tick-b.tick)
    return {...effect,sourceTextAnimation:{timeBase:1000000,keyframes}}
  }
  function editSourceKey(effect:GraphicText,tick:number,patch:{tick?:number;value?:string}|null):GraphicText {
    const previous=effect.sourceTextAnimation;if(!previous)return effect
    const keyframes=previous.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
    if(keyframes.some((key,i)=>!Number.isSafeInteger(key.tick)||(i>0&&key.tick===keyframes[i-1]!.tick)))throw new Error('Время ключей должно быть уникальным')
    return {...effect,sourceTextAnimation:keyframes.length?{...previous,keyframes}:undefined}
  }
  function editKey(effect: GraphicText, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): GraphicText {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="GraphicText">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'graphic_text', parameters: {} }])}>Добавить GraphicText</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'graphic_text'}
            {@const source=nestedComposition.document?.compositions[compositionId]?.content.sources?.[clip.sourceId]}
            {@const canvas=nestedComposition.document?.compositions[compositionId]?.content.canvas}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked || textBusy}>
              <legend>GraphicText {index + 1}</legend>

              <label>Название<input aria-label={`${clip.id} GraphicText name`} value={effect.parameters.name??''} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,name:event.currentTarget.value}}))} /></label>
              {#each flags as [channel,label,base]}<label><input type="checkbox" aria-label={`${clip.id} GraphicText ${channel}`} checked={effect.parameters[channel]??base} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:event.currentTarget.checked}}))} />{label}</label>{/each}
              {#each [['text','Текст',''],['font','Шрифт','Inter'],['fontStyle','Начертание','Regular']] as [field,label,base]}
              <label>{label}<input aria-label={`${clip.id} GraphicText ${field}`} value={effect.parameters[field as 'text'|'font'|'fontStyle']??base} onchange={event=>textField(track.id,clip.id,index,field,event.currentTarget.value,effect)} /></label>
              {/each}
              <GraphicCharacterFormatting text={effect.parameters.text??''} runs={effect.parameters.runs??[]} label={`${clip.id} GraphicText`} disabled={disabled||nestedHistory.busy||track.locked||textBusy} onchange={runs=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,runs}}))}/>
              {#if textError}<p role="alert">{textError}</p>{/if}
              <div class="row"><button class="btn ghost sm" aria-label={`${clip.id} GraphicText source key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>sourceKey(current,('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ текста у курсора</button>
              {#if effect.sourceTextAnimation}<button class="btn ghost sm" aria-label={`${clip.id} GraphicText source clear`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,sourceTextAnimation:undefined}))}>Очистить ключи текста</button>{/if}</div>
              {#if effect.sourceTextAnimation}
                <details><summary>Ключи текста · {effect.sourceTextAnimation.keyframes.length}</summary>
                {#each effect.sourceTextAnimation.keyframes as key (key.tick)}<div class="row">
                <label>Время, с<input type="number" step="0.001" aria-label={`${clip.id} GraphicText source ${key.tick} time`} value={key.tick/effect.sourceTextAnimation.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>editSourceKey(current,key.tick,{tick:Math.round(Number(event.currentTarget.value)*effect.sourceTextAnimation!.timeBase)}))}/></label>
                <label>Текст<input aria-label={`${clip.id} GraphicText source ${key.tick} value`} value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>editSourceKey(current,key.tick,{value:event.currentTarget.value}))}/></label>
                <button class="btn ghost sm" aria-label={`${clip.id} GraphicText source ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>editSourceKey(current,key.tick,null))}>Удалить ключ текста</button>
                </div>{/each}</details>
              {/if}
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} GraphicText ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={displayed(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value),source?.width??canvas?.width??0,source?.height??canvas?.height??0))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} GraphicText ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, automatic(channel,base,source?.width??canvas?.width??0,source?.height??canvas?.height??0), ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} GraphicText ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} GraphicText ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} GraphicText ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} GraphicText ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} GraphicText ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'graphic_text', parameters: {} }))}>Сбросить GraphicText</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить GraphicText</button></div>
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
