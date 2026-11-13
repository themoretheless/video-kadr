<script lang="ts">
  import {compositionProjectTemplateUrl} from '$lib/api'
  let { projectId, timeUs = 0, compositionId, disabled = false }: {
    projectId: string
    timeUs?: number
    compositionId?: string
    disabled?: boolean
  } = $props()
  let includePoster = $state(false)
  let message=$state(''),error=$state('')
  $effect(()=>{void projectId;message='';error=''})
  function download(){
    if(disabled)return
    message='';error=''
    const anchor=document.createElement('a')
    try{
      anchor.href = compositionProjectTemplateUrl(projectId, includePoster ? { timeUs, compositionId } : undefined)
      anchor.download = ''
      anchor.rel='noopener';document.body.append(anchor);anchor.click()
      message='Загрузка шаблона сохранённой версии началась'
    }catch(reason){error=reason instanceof Error?reason.message:String(reason)}
    finally{anchor.remove()}
  }
</script>
<label><input type="checkbox" bind:checked={includePoster} {disabled} /> Обложка из кадра сохранённой версии</label>
<button class="btn ghost sm" {disabled} onclick={download}>Скачать шаблон сохранённой версии</button>
{#if message}<span role="status">{message}</span>{/if}
{#if error}<span role="alert">{error}</span>{/if}
