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
  onfile: (file: File, dropTime?: number) => Promise<boolean>
  onerror: (message: string) => void
 } = $props()
 let dragging = $state(false)
 let filesInPanel = $state<File[]>([])
 let applying = $state(false)
 let stopDrag: (() => void) | undefined
 let poolOnly = $state(false)
 const stored = $derived(documents.map(document => ({
  id: document.id,
  file: new File([document.text], document.name),
 })))
 const entries = $derived([...stored, ...filesInPanel.map(file => ({ id: undefined, file }))])
 const blocked = $derived(disabled || applying)
 $effect(() => {
  void authState.token
  filesInPanel = []
  return () => stopDrag?.()
 })

 function remember(file: File) {
  if (filesInPanel.includes(file)) return
  filesInPanel = [file, ...filesInPanel.filter(item =>
   item.name !== file.name || item.size !== file.size || item.lastModified !== file.lastModified,
  )].slice(0, 8)
 }

 async function apply(file: File, addOnly = false, dropTime?: number) {
  if (blocked) return
  applying = true
  const owner = authState.token
  const originalProject = project
  const isStored = stored.some(entry => entry.file === file)
  try {
   if (addOnly) {
    onerror('')
    if (file.size > MAX_SRT_BYTES) throw new Error('Файл субтитров превышает лимит 2 МиБ')
    const body = await file.text()
    if (!parseSubtitleDocument(body).length) throw new Error('Файл не содержит субтитров')
    if (authState.token !== owner) return
    if (project !== originalProject) throw new Error('Проект изменился во время чтения файла')
    await ondocumentschange({ id: crypto.randomUUID(), name: file.name, text: body })
   } else if (await onfile(file, dropTime) && authState.token === owner && !isStored) {
    remember(file)
   }
  } catch (error) {
   if (authState.token === owner) onerror(error instanceof Error ? error.message : String(error))
  } finally {
   applying = false
  }
 }

 async function remove(file: File, id?: string) {
  if (blocked) return
  if (id === undefined) {
   filesInPanel = filesInPanel.filter(item => item !== file)
   return
  }
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
  void apply(file, poolOnly)
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
 {#if entries.length}
  <ul aria-label="Недавние файлы субтитров">
   {#each entries as { id, file } (id ?? file)}
    <li>
     <button class="btn ghost sm" type="button" disabled={blocked} draggable={!blocked}
      ondragstart={event => {
       stopDrag?.()
       if (blocked) { event.preventDefault(); return }
       stopDrag = startSubtitleDrag(event, time => {
        if (entries.some(entry => id === undefined ? entry.file === file : entry.id === id)) void apply(file, false, time)
       })
      }} ondragend={() => stopDrag?.()} onclick={() => void apply(file)}>
      Вставить · {file.name}
     </button>
     <button class="btn ghost sm" type="button" disabled={blocked} aria-label={`Убрать файл ${file.name} из списка`}
      onclick={() => void remove(file, id)}>Убрать</button>
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
