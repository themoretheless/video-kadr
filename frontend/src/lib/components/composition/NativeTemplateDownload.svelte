<script lang="ts">
  import {compositionProjectTemplateUrl, saveProjectTemplate} from '$lib/api'
  let { projectId, timeUs = 0, compositionId, token, onSaved, disabled = false }: {
    projectId: string
    timeUs?: number
    compositionId?: string
    token?: string | null
    onSaved?: () => void
    disabled?: boolean
  } = $props()
  let name = $state('')
  let includePoster = $state(false)
  let message=$state(''),error=$state('')
  $effect(()=>{void projectId;name='';message='';error=''})
  let busy = $state(false)
  let request: AbortController | undefined
  $effect(() => {
    void token
    void projectId
    return () => request?.abort()
  })
  async function save() {
    if (!token || busy || disabled) return
    const controller = new AbortController()
    request = controller
    busy = true
    message = ''
    error = ''
    try {
      const poster = includePoster ? { timeUs, compositionId } : undefined
      await saveProjectTemplate(projectId, token, poster, controller.signal, name.trim() || undefined)
      if (controller.signal.aborted) return
      message = 'Шаблон сохранён в библиотеку'
      onSaved?.()
    } catch (reason) {
      if (!controller.signal.aborted) error = reason instanceof Error ? reason.message : String(reason)
    } finally {
      if (request === controller) busy = false
    }
  }
  function download(){
    if(disabled)return
    message='';error=''
    const anchor=document.createElement('a')
    try{
      anchor.href = compositionProjectTemplateUrl(projectId, includePoster ? { timeUs, compositionId } : undefined, name.trim() || undefined)
      anchor.download = ''
      anchor.rel='noopener';document.body.append(anchor);anchor.click()
      message='Загрузка шаблона сохранённой версии началась'
    }catch(reason){error=reason instanceof Error?reason.message:String(reason)}
    finally{anchor.remove()}
  }
</script>
<label>Название шаблона <input type="text" bind:value={name} placeholder="Название проекта" disabled={disabled || busy} /></label>
<label><input type="checkbox" bind:checked={includePoster} {disabled} /> Обложка из кадра сохранённой версии</label>
<button class="btn ghost sm" {disabled} onclick={download}>Скачать шаблон сохранённой версии</button>
{#if token}
  <button class="btn ghost sm" disabled={disabled || busy} onclick={save}>
    {busy ? 'Сохраняю шаблон…' : 'Сохранить шаблон в библиотеку'}
  </button>
{/if}
{#if message}<span role="status">{message}</span>{/if}
{#if error}<span role="alert">{error}</span>{/if}
