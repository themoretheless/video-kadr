<script lang="ts">
  import { MAX_TEMPLATE_COVER_BYTES } from '$lib/api'

  let { action, disabled = false, form = $bindable(), file = $bindable() }: {
    action: string
    disabled?: boolean
    form?: HTMLFormElement
    file?: File
  } = $props()
  let error = $state('')

  function selectCover(event: Event & { currentTarget: HTMLInputElement }) {
    const selected = event.currentTarget.files?.[0]
    error = ''
    if (selected && (selected.size === 0 || selected.size > MAX_TEMPLATE_COVER_BYTES)) {
      error = 'Выберите непустой PNG или JPEG размером до 1 МиБ'
      event.currentTarget.value = ''
      file = undefined
      return
    }
    file = selected
  }
</script>

<form bind:this={form} method="post" enctype="multipart/form-data" target="_blank" rel="noopener" {action}>
  <label>Изображение обложки
    <input type="file" name="poster" accept="image/png,image/jpeg" required {disabled} onchange={selectCover} />
  </label>
  {#if error}<span role="alert">{error}</span>{/if}
</form>
