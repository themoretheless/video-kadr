<script lang="ts">
  import {untrack} from 'svelte'
  import type {WireTextClip} from '$lib/composition/types'
  import {nestedComposition,nestedAgentGeneration,nestedHistory,updateNestedCaptionText} from '$lib/state/nestedComposition.svelte'
  let {compositionId,clip,disabled=false}:{compositionId:string;clip:WireTextClip;disabled?:boolean}=$props()
  let draft=$state(''),base=$state(''),scope=$state(''),error=$state('')
  const changed=$derived(draft!==base)
  const conflict=$derived(base!==clip.text)
  $effect(()=>{
    const nextScope=`${nestedComposition.projectId}/${nestedAgentGeneration()}/${compositionId}/${clip.id}`,text=clip.text
    untrack(()=>{if(scope!==nextScope||draft===base){scope=nextScope;base=text;draft=text;error=''}})
  })
  function reset(){base=clip.text;draft=clip.text;error=''}
  async function save(){
    error=''
    const submitted=draft,startedScope=scope
    const accepted=await updateNestedCaptionText(compositionId,clip.id,submitted,base)
    if(scope!==startedScope)return
    if(accepted)base=submitted
    else error=nestedHistory.error||'Сохранение отменено: проект изменился'
  }
</script>
<details data-caption-editor={clip.id}>
  <summary>Редактировать реплику</summary>
  <label>Текст субтитра<textarea bind:value={draft} disabled={disabled||nestedHistory.busy}></textarea></label>
  <span>{Array.from(draft).length} / 512</span>
  {#if conflict}<p role="alert">Реплика изменилась после начала редактирования. Сбросьте черновик, чтобы загрузить актуальный текст.</p>{/if}
  <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||!changed||conflict} onclick={()=>void save()}>Сохранить текст</button>
  <button class="btn ghost sm" type="button" disabled={nestedHistory.busy} onclick={reset}>Сбросить изменения текста</button>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
