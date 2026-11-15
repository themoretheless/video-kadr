<script lang="ts">
 import { startSubtitleDrag } from '$lib/subtitles/timelineDrop'
 import { authState } from '$lib/state/auth.svelte'
 import type { SubtitleDocument } from '$lib/subtitles/documents'
 import { parseSubtitleDocument } from '$lib/subtitles/import'
 import { MAX_SRT_BYTES } from '$lib/subtitles/srt'

 let { label, disabled = false, documents = [], project, ondocumentschange, onfile, onerror }: {
  label: string
  disabled?: boolean
  project: object | null
  documents?: readonly SubtitleDocument[]
  ondocumentschange: (change: SubtitleDocument | string) => void | Promise<void>
  onfile: (file: File, dropTime?: number, saveSource?: boolean) => Promise<boolean>
  onerror: (message: string) => void
 } = $props()
 let dragging = $state(false)
 let applying = $state(false)
 let stopDrag: (() => void) | undefined
 let poolOnly = $state(false)
 const blocked = $derived(disabled || applying)
 $effect(() => {
  void authState.token
  return () => stopDrag?.()
 })

 async function apply(file: File, action: 'stage' | 'import' | 'insert', dropTime?: number) {
  if (blocked) return
  applying = true
  const owner = authState.token
  const originalProject = project
  try {
   if (action === 'stage') {
    onerror('')
    if (file.size > MAX_SRT_BYTES) throw new Error('Файл субтитров превышает лимит 2 МиБ')
    const body = await file.text()
    if (!parseSubtitleDocument(body).length) throw new Error('Файл не содержит субтитров')
    if (authState.token !== owner) return
    if (project !== originalProject) throw new Error('Проект изменился во время чтения файла')
    await ondocumentschange({ id: crypto.randomUUID(), name: file.name, text: body })
   } else {
    await onfile(file, dropTime, action === 'import')
   }
  } catch (error) {
   if (authState.token === owner) onerror(error instanceof Error ? error.message : String(error))
  } finally {
   applying = false
  }
 }

 async function remove(id: string) {
  if (blocked) return
  const owner = authState.token
  applying = true
  try {
   await ondocumentschange(id)
  } catch (error) {
   if (authState.token === owner) onerror(error instanceof Error ? error.message : String(error))
  } finally {
   applying = false
  }
 }

 function insert(id: string, dropTime?: number) {
  const document = documents.find(item => item.id === id)
  if (document) void apply(new File([document.text], document.name), 'insert', dropTime)
 }

 function select(files: FileList | null | undefined, dropped = false) {
  if (blocked || !files?.length) return
  if (files.length !== 1) {
   onerror('Выберите один файл субтитров')
   return
  }
  const file = files[0]!
  if (dropped && !/\.(?:srt|vtt|ass|ssa|txt)$/i.test(file.name)) {
   onerror('Выберите файл SRT, VTT, ASS, SSA или TXT')
   return
  }
  void apply(file, poolOnly ? 'stage' : 'import')
 }
</script>

<div class="subtitle-picker" class:dragging role="group" aria-label="Область импорта субтитров"
 ondragover={event => {
  if (!event.dataTransfer?.types.includes('Files')) return
  event.preventDefault()
  event.dataTransfer.dropEffect = blocked ? 'none' : 'copy'
  dragging = !blocked
 }}
 ondragleave={event => {
  if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) dragging = false
 }}
 ondrop={event => {
  event.preventDefault()
  dragging = false
  select(event.dataTransfer?.files, true)
 }}>
 <label class="pool-option"><input type="checkbox" bind:checked={poolOnly} disabled={blocked} />Добавить файл в список без вставки на таймлайн</label>
 <label>Выберите или перетащите файл субтитров
  <input type="file" aria-label={label} accept=".srt,.vtt,.ass,.ssa,.txt,application/x-subrip,text/vtt,text/plain" disabled={blocked}
   onchange={event => {
    select(event.currentTarget.files)
    event.currentTarget.value = ''
   }} />
 </label>
 {#if documents.length}
  <ul aria-label="Недавние файлы субтитров">
   {#each documents as { id, name } (id)}
    <li>
     <button class="btn ghost sm" type="button" disabled={blocked} draggable={!blocked}
      ondragstart={event => {
       stopDrag?.()
       if (blocked) { event.preventDefault(); return }
       const originalProject = project
       stopDrag = startSubtitleDrag(event, time => {
        if (project === originalProject) insert(id, time)
       })
      }} ondragend={() => stopDrag?.()} onclick={() => insert(id)}>
      Вставить · {name}
     </button>
     <button class="btn ghost sm" type="button" disabled={blocked} aria-label={`Убрать файл ${name} из списка`}
      onclick={() => void remove(id)}>Убрать</button>
    </li>
   {/each}
  </ul>
 {/if}
</div>

<style>
 .subtitle-picker { border: 1px dashed currentColor; border-radius: 0.375rem; padding: 0.5rem; }
 .subtitle-picker.dragging { outline: 2px solid currentColor; }
 label { display: grid; gap: 0.25rem; }
 .pool-option { display: flex; align-items: center; margin-bottom: 0.25rem; }
 input { max-width: 100%; }
 ul { list-style: none; padding: 0; margin: 0.5rem 0 0; }
 li { display: flex; flex-wrap: wrap; gap: 0.25rem; }
</style>
