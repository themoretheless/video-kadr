<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { editFilmcraftMetadata, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'

  let { document, item, sequenceId, clipId, token, palette, disabled, onedit }: {
    document: string
    item: NonNullable<FilmcraftImportResult['projectItems']>[number] | undefined
    sequenceId: string
    clipId: string
    token?: string | null
    palette: NonNullable<FilmcraftImportResult['labelPalette']>
    disabled: boolean
    onedit: (edited: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean
  } = $props()

  const logFields = ['Description', 'Scene', 'Shot', 'Log Note', 'Comment', 'Tape Name', 'Client', 'Camera Angle']
  const newField = ''
  const fields = $derived([...new SvelteSet(['Name', 'Label', ...logFields, ...Object.keys(item?.iconSort?.metadata ?? {})])])
  let field = $state('Name')
  let customName = $state('')
  let draft = $state('')
  let error = $state('')
  let busy = $state(false)

  $effect(() => {
    const name = item?.name ?? ''
    const label = item?.label ?? ''
    const metadata = item?.iconSort?.metadata
    if (field !== newField && !fields.includes(field)) field = 'Name'
    if (field === newField) draft = ''
    else if (field === 'Name') draft = name
    else if (field === 'Label') draft = label
    else draft = Object.hasOwn(metadata ?? {}, field) ? metadata?.[field] ?? '' : ''
  })

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    const key = field === newField ? customName.trim() : field
    if (!key || !item || !token || disabled || busy) return
    const expected = { document, sequenceId, clipId, token }
    const target = item.id
    busy = true
    error = ''
    try {
      const edited = await editFilmcraftMetadata(token, { document, item: target, field: key, value: draft })
      if (edited && item?.id === target && onedit(edited, expected) && field === newField) {
        field = key
        customName = ''
      }
    } catch (failure) {
      if (item?.id === target && document === expected.document) error = failure instanceof Error ? failure.message : String(failure)
    } finally {
      busy = false
    }
  }
</script>

{#if item}
  <details>
    <summary>Метаданные Filmcraft</summary>
    <form onsubmit={submit}>
      <span>{item.name}</span>
      <label>Поле метаданных
        <select aria-label="Поле метаданных" bind:value={field} disabled={disabled || busy}>
          {#each fields as name (name)}<option value={name}>{name}</option>{/each}
          <option value={newField}>Пользовательское поле</option>
        </select>
      </label>
      {#if field === newField}
        <label>Имя поля <input bind:value={customName} disabled={disabled || busy} required /></label>
      {/if}
      <label>Значение метаданных
        {#if field === 'Label'}
          <select aria-label="Значение метаданных" bind:value={draft} disabled={disabled || busy}>
            {#each palette as label (label.name)}<option value={label.name}>{label.name}</option>{/each}
          </select>
        {:else}
          <input bind:value={draft} disabled={disabled || busy} required={field === 'Name'} />
        {/if}
      </label>
      <button type="submit" disabled={disabled || busy || !token}>Сохранить метаданные</button>
    </form>
    {#if error}<p role="alert">{error}</p>{/if}
  </details>
{/if}

<style>
  form { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
  summary { cursor: pointer; }
</style>
