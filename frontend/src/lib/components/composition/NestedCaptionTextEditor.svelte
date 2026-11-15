<script lang="ts">
  import {untrack} from 'svelte'
  import type {WireTextClip} from '$lib/composition/types'
  import {captionTypography, sameCaptionTypography, type CaptionTypography} from '$lib/composition/nativeCaptionText'
  import {nestedComposition,nestedAgentGeneration,nestedHistory,updateNestedCaptionText} from '$lib/state/nestedComposition.svelte'
  let {compositionId,clip,disabled=false}:{compositionId:string;clip:WireTextClip;disabled?:boolean}=$props()
  let draft=$state(''),base=$state(''),scope=$state(''),error=$state('')
  let typography=$state<{ -readonly [Field in keyof CaptionTypography]: CaptionTypography[Field] }>({fontSize: 24})
  let baseTypography=$state<CaptionTypography>({fontSize: 24})
  const changed=$derived(draft!==base || !sameCaptionTypography(typography, baseTypography))
  const conflict=$derived(base!==clip.text || !sameCaptionTypography(baseTypography, clip.style))
  $effect(()=>{
    const nextScope=`${nestedComposition.projectId}/${nestedAgentGeneration()}/${compositionId}/${clip.id}`,text=clip.text
    const style=captionTypography(clip.style)
    untrack(()=>{
      if(scope!==nextScope||!changed){
        scope=nextScope;base=text;draft=text;error=''
        baseTypography=style;typography={...style}
      }
    })
  })
  function reset(){
    base=clip.text;draft=clip.text;error=''
    baseTypography=captionTypography(clip.style);typography={...baseTypography}
  }
  async function save(){
    error=''
    const submitted=draft,startedScope=scope
    const submittedTypography={...typography}
    const accepted=await updateNestedCaptionText(compositionId,clip.id,submitted,base,
      clip.concatTitle ? undefined : {value:submittedTypography,expected:{...baseTypography}})
    if(scope!==startedScope)return
    if(accepted){base=submitted;baseTypography=submittedTypography}
    else error=nestedHistory.error||'Сохранение отменено: проект изменился'
  }
</script>
<details data-caption-editor={clip.id}>
  <summary>Редактировать реплику</summary>
  <label>Текст субтитра<textarea bind:value={draft} disabled={disabled||nestedHistory.busy}></textarea></label>
  <span>{Array.from(draft).length} / 512</span>
  {#if !clip.concatTitle}
    <label>Размер шрифта, px<input type="number" min="1" max="1000" step="any" bind:value={typography.fontSize} disabled={disabled||nestedHistory.busy} /></label>
    <label>Вес шрифта<input type="number" min="100" max="900" step="any" placeholder={clip.style.bold ? '700' : '400'} bind:value={typography.fontWeight} disabled={disabled||nestedHistory.busy} /></label>
    <label>Межбуквенный интервал, px<input type="number" step="any" placeholder="0" bind:value={typography.tracking} disabled={disabled||nestedHistory.busy} /></label>
    <label>Высота строки, ×<input type="number" min="0.5" step="any" placeholder="Авто" bind:value={typography.lineHeight} disabled={disabled||nestedHistory.busy} /></label>
  {/if}
  {#if conflict}<p role="alert">Реплика изменилась после начала редактирования. Сбросьте черновик, чтобы загрузить актуальный текст.</p>{/if}
  <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||!changed||conflict} onclick={()=>void save()}>Сохранить реплику</button>
  <button class="btn ghost sm" type="button" disabled={nestedHistory.busy||(!changed&&!conflict)} onclick={reset}>Сбросить черновик</button>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
