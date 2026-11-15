<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { editFilmcraftMetadata, getFilmcraftMetadata, type FilmcraftImportResult, type FilmcraftMetadataField, type FilmcraftSourceEditFence } from '$lib/api'

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
  let inspect = $state(false)
  let properties = $state<FilmcraftMetadataField[]>([])
  let inspectError = $state('')
  let inspecting = $state(false)

  $effect(() => {
    const target = item?.id
    properties = []
    inspectError = ''
    inspecting = false
    if (!inspect || !target || !token) return
    const request = new AbortController()
    inspecting = true
    void getFilmcraftMetadata(token, { document, item: target }, request.signal).then(
      result => { if (!request.signal.aborted) properties = result.filter(field => !field.editable) },
      failure => { if (!request.signal.aborted) inspectError = failure instanceof Error ? failure.message : String(failure) },
    ).finally(() => { if (!request.signal.aborted) inspecting = false })
    return () => request.abort()
  })

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
    {#if token}
      <details ontoggle={event => { inspect = event.currentTarget.open }}>
        <summary>Свойства клипа и файла</summary>
        {#if inspecting}<p role="status">Загрузка свойств…</p>{/if}
        {#if inspectError}<p role="alert">{inspectError}</p>{/if}
        {#each ['Clip', 'File'] as section (section)}
          {@const rows = properties.filter(field => field.section === section)}
          {#if rows.length}
            <h4>{section === 'Clip' ? 'Клип' : 'Файл'}</h4>
            <dl>
              {#each rows as row (row.name)}
                <dt>{row.name}</dt><dd>{row.value || '—'}</dd>
              {/each}
            </dl>
          {/if}
        {/each}
      </details>
    {/if}
  </details>
{/if}

<style>
  form { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
  summary { cursor: pointer; }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 0.25rem 0.75rem; }
  dd { margin: 0; overflow-wrap: anywhere; }
</style>
