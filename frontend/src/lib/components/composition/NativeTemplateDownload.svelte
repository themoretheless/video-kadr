<script lang="ts">
  import {compositionProjectTemplateUrl} from '$lib/api'
  let {projectId,disabled=false}:{projectId:string;disabled?:boolean}=$props()
  let message=$state(''),error=$state('')
  $effect(()=>{void projectId;message='';error=''})
  function download(){
    if(disabled)return
    message='';error=''
    const anchor=document.createElement('a')
    try{
      anchor.href=compositionProjectTemplateUrl(projectId)
      anchor.rel='noopener';document.body.append(anchor);anchor.click()
      message='Загрузка шаблона сохранённой версии началась'
    }catch(reason){error=reason instanceof Error?reason.message:String(reason)}
    finally{anchor.remove()}
  }
</script>
<button class="btn ghost sm" {disabled} onclick={download}>Скачать шаблон сохранённой версии</button>
{#if message}<span role="status">{message}</span>{/if}
{#if error}<span role="alert">{error}</span>{/if}
