<script lang="ts">
 let { value, trackName, disabled = false, onchange }: {
  value?: number
  trackName?: string
  disabled?: boolean
  onchange: (width: number | undefined) => void
 } = $props()
 const percent = $derived(value === undefined ? '' : String(Math.round(value * 1000) / 10))

 function change(event: Event) {
  const input = event.currentTarget as HTMLInputElement
  const width = input.valueAsNumber / 100
  if (input.validity.badInput || (input.value !== '' && (!Number.isFinite(width) || width < 0.1 || width > 1))) {
   input.value = percent
   return
  }
  onchange(input.value === '' ? undefined : width)
 }
</script>

<label>
 {trackName ? `${trackName} · перенос строк, % кадра` : 'Перенос строк, % кадра'}
 <input type="number" aria-label={trackName ? `Ширина переноса субтитров: ${trackName}` : 'Ширина переноса субтитров'}
  min="10" max="100" step="0.1" placeholder="90" value={percent} {disabled} onchange={change} />
</label>
