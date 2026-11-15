<script lang="ts">
 import { authState } from '$lib/state/auth.svelte'

 let { label, disabled = false, onfile, onerror }: {
  label: string
  disabled?: boolean
  onfile: (file: File) => Promise<boolean>
  onerror: (message: string) => void
 } = $props()
 let dragging = $state(false)
 let filesInPanel = $state<File[]>([])
 let applying = $state(false)
 const blocked = $derived(disabled || applying)
 $effect(() => {
  void authState.token
  filesInPanel = []
 })

 async function apply(file: File) {
  if (blocked) return
  applying = true
  const owner = authState.token
  try {
   if (await onfile(file) && authState.token === owner && !filesInPanel.includes(file)) {
    filesInPanel = [file, ...filesInPanel.filter(item =>
     item.name !== file.name || item.size !== file.size || item.lastModified !== file.lastModified,
    )].slice(0, 8)
   }
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
  void apply(file)
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
 <label>Выберите или перетащите файл субтитров
  <input type="file" aria-label={label} accept=".srt,.vtt,.ass,.ssa,.txt,application/x-subrip,text/vtt,text/plain" disabled={blocked}
   onchange={event => {
    select(event.currentTarget.files)
    event.currentTarget.value = ''
   }} />
 </label>
 {#if filesInPanel.length}
  <ul aria-label="Недавние файлы субтитров">
   {#each filesInPanel as file (file)}
    <li>
     <button class="btn ghost sm" type="button" disabled={blocked} onclick={() => void apply(file)}>
      Вставить снова · {file.name}
     </button>
     <button class="btn ghost sm" type="button" disabled={blocked} aria-label={`Убрать файл ${file.name} из списка`}
      onclick={() => { filesInPanel = filesInPanel.filter(item => item !== file) }}>Убрать</button>
    </li>
   {/each}
  </ul>
 {/if}
</div>

<style>
 .subtitle-picker { border: 1px dashed currentColor; border-radius: 0.375rem; padding: 0.5rem; }
 .subtitle-picker.dragging { outline: 2px solid currentColor; }
 label { display: grid; gap: 0.25rem; }
 input { max-width: 100%; }
 ul { list-style: none; padding: 0; margin: 0.5rem 0 0; }
 li { display: flex; flex-wrap: wrap; gap: 0.25rem; }
</style>
