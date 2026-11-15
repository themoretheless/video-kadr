<script lang="ts">
  import {untrack} from 'svelte'
  import type {WireTextClip} from '$lib/composition/types'
  import {captionAppearance, sameCaptionAppearance, type CaptionAppearance} from '$lib/composition/nativeCaptionText'
  import {nestedComposition,nestedAgentGeneration,nestedHistory,updateNestedCaptionText} from '$lib/state/nestedComposition.svelte'
  const fontsId = $props.id()
  let {compositionId,clip,disabled=false}:{compositionId:string;clip:WireTextClip;disabled?:boolean}=$props()
  let draft=$state(''),base=$state(''),scope=$state(''),error=$state('')
  const defaultAppearance = {
    fontFamily: 'Noto Sans', fontSize: 24,
    textColor: '#ffffff', textAlpha: 1, backgroundColor: '#000000', backgroundAlpha: 0,
    strokeColor: '#000000', strokeAlpha: 0, strokeWidth: 0,
    shadowColor: '#000000', shadowAlpha: 0, shadowX: 0, shadowY: 0,
  }
  let appearance=$state<{ -readonly [Field in keyof CaptionAppearance]: CaptionAppearance[Field] }>({...defaultAppearance})
  let baseAppearance=$state<CaptionAppearance>({...defaultAppearance})
  const families = $derived([...new Set([
    ...(nestedComposition.document?.fonts ?? []).map(font => font.family),
    'Noto Sans', 'Inter', 'Arial', 'JetBrains Mono', 'Noto Serif',
  ])])
  const changed=$derived(draft!==base || !sameCaptionAppearance(appearance, baseAppearance))
  const conflict=$derived(base!==clip.text || !sameCaptionAppearance(baseAppearance, captionAppearance(clip)))
  const shadowMode=$derived(appearance.concatShadow ? 'concat' : appearance.shadowAlpha === 0 ? 'off' : 'custom')
  function changeShadowMode(mode: string) {
    appearance.concatShadow=mode==='concat' || undefined
    if(mode==='off')appearance.shadowAlpha=0
    else if(mode==='custom' && appearance.shadowAlpha===0)appearance.shadowAlpha=0.6
  }
  $effect(()=>{
    const nextScope=`${nestedComposition.projectId}/${nestedAgentGeneration()}/${compositionId}/${clip.id}`,text=clip.text
    const style=captionAppearance(clip)
    untrack(()=>{
      if(scope!==nextScope||!changed){
        scope=nextScope;base=text;draft=text;error=''
        baseAppearance=style;appearance={...style}
      }
    })
  })
  function reset(){
    base=clip.text;draft=clip.text;error=''
    baseAppearance=captionAppearance(clip);appearance={...baseAppearance}
  }
  async function save(){
    error=''
    const submitted=draft,startedScope=scope
    const submittedAppearance={...appearance, fontFamily:appearance.fontFamily.trim()}
    if(submittedAppearance.paddingX!==undefined || submittedAppearance.paddingY!==undefined){
      submittedAppearance.paddingX??=0
      submittedAppearance.paddingY??=0
    }
    const accepted=await updateNestedCaptionText(compositionId,clip.id,submitted,base,
      clip.concatTitle ? undefined : {value:submittedAppearance,expected:{...baseAppearance}})
    if(scope!==startedScope)return
    if(accepted){base=submitted;baseAppearance=submittedAppearance;appearance={...submittedAppearance}}
    else error=nestedHistory.error||'Сохранение отменено: проект изменился'
  }
</script>
<details data-caption-editor={clip.id}>
  <summary>Редактировать реплику</summary>
  <label>Текст субтитра<textarea bind:value={draft} disabled={disabled||nestedHistory.busy}></textarea></label>
  <span>{Array.from(draft).length} / 512</span>
  {#if !clip.concatTitle}
    <label>Семейство шрифта<input type="text" list={fontsId} bind:value={appearance.fontFamily} disabled={disabled||nestedHistory.busy} /></label>
    <datalist id={fontsId}>
      {#each families as family (family)}<option value={family}></option>{/each}
    </datalist>
    <label>Размер шрифта, px<input type="number" min="1" max="1000" step="any" bind:value={appearance.fontSize} disabled={disabled||nestedHistory.busy} /></label>
    <label>Вес шрифта<input type="number" min="100" max="900" step="any" placeholder={clip.style.bold ? '700' : '400'} bind:value={appearance.fontWeight} disabled={disabled||nestedHistory.busy} /></label>
    <label><input type="checkbox" checked={appearance.italic === true} onchange={event=>{appearance.italic=event.currentTarget.checked || undefined}} disabled={disabled||nestedHistory.busy} /> Курсив</label>
    <label>Выравнивание
      <select value={appearance.align ?? 'left'} onchange={event=>{appearance.align=event.currentTarget.value as CaptionAppearance['align']}} disabled={disabled||nestedHistory.busy}>
        <option value="left">По левому краю</option>
        <option value="center">По центру</option>
        <option value="right">По правому краю</option>
      </select>
    </label>
    <label>Межбуквенный интервал, px<input type="number" step="any" placeholder="0" bind:value={appearance.tracking} disabled={disabled||nestedHistory.busy} /></label>
    <label>Высота строки, ×<input type="number" min="0.5" step="any" placeholder="Авто" bind:value={appearance.lineHeight} disabled={disabled||nestedHistory.busy} /></label>
    <details>
      <summary>Размер блока и фон</summary>
      <label>Ширина блока, px<input type="number" min="1" max="100000" step="any" placeholder="Авто" bind:value={appearance.textBoxWidth} disabled={disabled||nestedHistory.busy} /></label>
      <label>Высота блока, px<input type="number" min="1" max="100000" step="any" placeholder="Авто" bind:value={appearance.textBoxHeight} disabled={disabled||nestedHistory.busy} /></label>
      <label>Цвет фона<input type="color" bind:value={appearance.backgroundColor} disabled={disabled||nestedHistory.busy} /></label>
      <label>Непрозрачность фона<input type="number" min="0" max="1" step="0.01" bind:value={appearance.backgroundAlpha} disabled={disabled||nestedHistory.busy} /></label>
      <label>Скругление фона, px<input type="number" min="0" max="1000" step="any" placeholder="0" bind:value={appearance.backgroundRadius} disabled={disabled||nestedHistory.busy} /></label>
      <label>Отступ фона по горизонтали, px<input type="number" min="0" max="100000" step="any" placeholder="0" bind:value={appearance.paddingX} disabled={disabled||nestedHistory.busy} /></label>
      <label>Отступ фона по вертикали, px<input type="number" min="0" max="100000" step="any" placeholder="0" bind:value={appearance.paddingY} disabled={disabled||nestedHistory.busy} /></label>
    </details>
    <details>
      <summary>Тень текста</summary>
      <label>Режим тени
        <select value={shadowMode} onchange={event=>changeShadowMode(event.currentTarget.value)} disabled={disabled||nestedHistory.busy}>
          <option value="off">Без тени</option>
          <option value="concat">Стиль Concat</option>
          <option value="custom">Своя тень</option>
        </select>
      </label>
      {#if shadowMode==='custom'}
        <label>Цвет тени<input type="color" bind:value={appearance.shadowColor} disabled={disabled||nestedHistory.busy} /></label>
        <label>Непрозрачность тени<input type="number" min="0" max="1" step="any" bind:value={appearance.shadowAlpha} disabled={disabled||nestedHistory.busy} /></label>
        <label>Смещение тени по горизонтали, px<input type="number" step="1" bind:value={appearance.shadowX} disabled={disabled||nestedHistory.busy} /></label>
        <label>Смещение тени по вертикали, px<input type="number" step="1" bind:value={appearance.shadowY} disabled={disabled||nestedHistory.busy} /></label>
      {/if}
    </details>
    <details>
      <summary>Цвет текста и обводка</summary>
      {#if clip.colorAnimation}<p>В кадрах с анимацией цвет текста задают ключи.</p>{/if}
      <label>Цвет текста<input type="color" bind:value={appearance.textColor} disabled={disabled||nestedHistory.busy} /></label>
      <label>Непрозрачность текста<input type="number" min="0" max="1" step="any" bind:value={appearance.textAlpha} disabled={disabled||nestedHistory.busy} /></label>
      <label>Цвет обводки<input type="color" bind:value={appearance.strokeColor} disabled={disabled||nestedHistory.busy} /></label>
      <label>Непрозрачность обводки<input type="number" min="0" max="1" step="any" bind:value={appearance.strokeAlpha} disabled={disabled||nestedHistory.busy} /></label>
      <label>Толщина обводки, px<input type="number" min="0" max="100" step="1" bind:value={appearance.strokeWidth} disabled={disabled||nestedHistory.busy} /></label>
    </details>
  {/if}
  {#if conflict}<p role="alert">Реплика изменилась после начала редактирования. Сбросьте черновик, чтобы загрузить актуальную версию.</p>{/if}
  <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||!changed||conflict} onclick={()=>void save()}>Сохранить реплику</button>
  <button class="btn ghost sm" type="button" disabled={nestedHistory.busy||(!changed&&!conflict)} onclick={reset}>Сбросить черновик</button>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
