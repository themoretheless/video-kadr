<script lang="ts">
  import TemplateCoverForm from './TemplateCoverForm.svelte'
  import {
    compositionProjectTemplateCoverUrl, compositionProjectTemplateUrl,
    saveProjectTemplate, saveProjectTemplateWithCover,
  } from '$lib/api'
  let { projectId, spaceId, timeUs = 0, compositionId, token, onSaved, hasSavedCover = false, disabled = false }: {
    projectId: string
    spaceId?: string
    timeUs?: number
    compositionId?: string
    token?: string | null
    onSaved?: () => void
    hasSavedCover?: boolean
    disabled?: boolean
  } = $props()
  let name = $state('')
  let saveToSpace = $state(false)
  $effect(() => { void spaceId; saveToSpace = false })
  let posterSource = $state<'' | 'frame' | 'saved' | 'file' | 'auto'>('')
  const poster = $derived(posterSource === 'frame' ? { timeUs, compositionId }
    : posterSource === 'auto' ? { automatic: true as const, compositionId }
    : posterSource === 'saved' && hasSavedCover ? 'saved' : undefined)
  let coverFile = $state<File>()
  let downloadForm = $state<HTMLFormElement>()
  const coverMissing = $derived(posterSource === 'file' && !coverFile)
  let message=$state(''),error=$state('')
  $effect(()=>{void projectId;name='';posterSource='';coverFile=undefined;message='';error=''})
  let busy = $state(false)
  let request: AbortController | undefined
  $effect(() => {
    if (!hasSavedCover && posterSource === 'saved') posterSource = ''
  })
  $effect(() => {
    void token
    void projectId
    void spaceId
    return () => request?.abort()
  })
  async function save() {
    if (!token || busy || disabled || coverMissing) return
    const destination = saveToSpace ? spaceId : undefined
    const controller = new AbortController()
    request = controller
    busy = true
    message = ''
    error = ''
    try {
      if (posterSource === 'file' && coverFile) {
        await saveProjectTemplateWithCover(projectId, token, coverFile, controller.signal, name.trim() || undefined, destination)
      } else {
        await saveProjectTemplate(projectId, token, poster, controller.signal, name.trim() || undefined, destination)
      }
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
    if(disabled || busy || coverMissing)return
    message='';error=''
    if (posterSource === 'file') {
      if (!downloadForm) return
      downloadForm.requestSubmit()
      message = 'Запрос на скачивание шаблона отправлен'
      return
    }
    const anchor=document.createElement('a')
    try{
      anchor.href = compositionProjectTemplateUrl(projectId, poster, name.trim() || undefined)
      anchor.download = ''
      anchor.rel='noopener';document.body.append(anchor);anchor.click()
      message='Загрузка шаблона сохранённой версии началась'
    }catch(reason){error=reason instanceof Error?reason.message:String(reason)}
    finally{anchor.remove()}
  }
</script>
{#if spaceId}
  <label>Сохранить шаблон в
    <select aria-label="Сохранить шаблон в" value={saveToSpace ? 'space' : 'personal'} disabled={disabled || busy}
      onchange={event => saveToSpace = event.currentTarget.value === 'space'}>
      <option value="personal">Личный каталог</option>
      <option value="space">Рабочее пространство</option>
    </select>
  </label>
{/if}
<label>Название шаблона <input type="text" bind:value={name} placeholder="Название проекта" disabled={disabled || busy} /></label>
<label>Обложка шаблона
  <select value={posterSource} disabled={disabled || busy} onchange={(event) => {
    const value = event.currentTarget.value
    coverFile = undefined
    error = ''
    posterSource = value === 'frame' || value === 'saved' || value === 'file' || value === 'auto' ? value : ''
  }}>
    <option value="">Без обложки</option>
    <option value="frame">Текущий кадр сохранённой версии</option>
    {#if hasSavedCover}<option value="saved">Обложка проекта</option>{/if}
    <option value="auto">Автоматически</option>
    <option value="file">Выбрать файл</option>
  </select>
</label>
{#if posterSource === 'file'}
  <TemplateCoverForm bind:form={downloadForm} bind:file={coverFile}
    disabled={disabled || busy}
    action={compositionProjectTemplateCoverUrl(projectId, name.trim() || undefined, true)} />
{/if}
<button class="btn ghost sm" disabled={disabled || busy || coverMissing} onclick={download}>Скачать шаблон сохранённой версии</button>
{#if token}
  <button class="btn ghost sm" disabled={disabled || busy || coverMissing} onclick={save}>
    {busy ? 'Сохраняю шаблон…' : 'Сохранить шаблон в библиотеку'}
  </button>
{/if}
{#if message}<span role="status">{message}</span>{/if}
{#if error}<span role="alert">{error}</span>{/if}
