<script lang="ts">
 let { label, disabled = false, onfile, onerror }: {
  label: string
  disabled?: boolean
  onfile: (file: File) => void
  onerror: (message: string) => void
 } = $props()
 let dragging = $state(false)

 function select(files: FileList | null | undefined, dropped = false) {
  if (disabled || !files?.length) return
  if (files.length !== 1) {
   onerror('Выберите один файл субтитров')
   return
  }
  const file = files[0]!
  if (dropped && !/\.(?:srt|vtt|ass|ssa|txt)$/i.test(file.name)) {
   onerror('Выберите файл SRT, VTT, ASS, SSA или TXT')
   return
  }
  onfile(file)
 }
</script>

<div class="subtitle-picker" class:dragging role="group" aria-label="Область импорта субтитров"
 ondragover={event => {
  if (!event.dataTransfer?.types.includes('Files')) return
  event.preventDefault()
  event.dataTransfer.dropEffect = disabled ? 'none' : 'copy'
  dragging = !disabled
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
  <input type="file" aria-label={label} accept=".srt,.vtt,.ass,.ssa,.txt,application/x-subrip,text/vtt,text/plain" {disabled}
   onchange={event => {
    select(event.currentTarget.files)
    event.currentTarget.value = ''
   }} />
 </label>
</div>

<style>
 .subtitle-picker { border: 1px dashed currentColor; border-radius: 0.375rem; padding: 0.5rem; }
 .subtitle-picker.dragging { outline: 2px solid currentColor; }
 label { display: grid; gap: 0.25rem; }
 input { max-width: 100%; }
</style>
