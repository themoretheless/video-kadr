<script lang="ts">
  import type { BrandColorDto, BrandKitPayloadDto } from '$lib/api.js'
  import { authState } from '$lib/state/auth.svelte.js'
  import { spacesState } from '$lib/state/spaces.svelte.js'
  import { state as legacyState } from '$lib/state/store.svelte.js'
  import { addLibraryEntryToComposition } from '$lib/state/composition.svelte.js'
  import {
    clearSpaceBrandKit,
    loadSpaceBrandKit,
    saveSpaceBrandKit,
    spaceBrandState,
  } from '$lib/state/spaceBrand.svelte.js'

  const FONTS = ['Noto Sans', 'Arial Unicode MS', 'DejaVu Sans', 'Arial'] as const
  let colors = $state<BrandColorDto[]>([])
  let fonts = $state<BrandKitPayloadDto['fonts']>([])
  let logoSourceIds = $state<string[]>([])
  let message = $state('')
  const imageSources = $derived(legacyState.library.filter((entry) => entry.kind === 'source' && entry.mediaType === 'image'))

  $effect(() => {
    const token = authState.token
    const spaceId = spacesState.selectedId
    if (token && spaceId) void loadSpaceBrandKit(spaceId, token)
    else clearSpaceBrandKit()
  })

  $effect(() => {
    if (spaceBrandState.spaceId !== spacesState.selectedId) return
    colors = spaceBrandState.kit.colors.map((color) => ({ ...color }))
    fonts = [...spaceBrandState.kit.fonts]
    logoSourceIds = [...spaceBrandState.kit.logoSourceIds]
  })

  function addColor(): void {
    if (colors.length >= 32) return
    colors = [...colors, { name: `Цвет ${colors.length + 1}`, value: '#3366ff' }]
  }

  function updateColor(index: number, patch: Partial<BrandColorDto>): void {
    colors = colors.map((color, candidate) => candidate === index ? { ...color, ...patch } : color)
  }

  function toggleFont(font: typeof FONTS[number], checked: boolean): void {
    fonts = checked ? [...fonts, font] : fonts.filter((candidate) => candidate !== font)
  }

  function toggleLogo(sourceId: string, checked: boolean): void {
    logoSourceIds = checked ? [...logoSourceIds, sourceId] : logoSourceIds.filter((id) => id !== sourceId)
  }

  async function save(): Promise<void> {
    const token = authState.token
    const spaceId = spacesState.selectedId
    if (!token || !spaceId || spaceBrandState.loading) return
    message = ''
    try {
      await saveSpaceBrandKit(spaceId, token, { colors, fonts, logoSourceIds })
      message = 'Бренд-кит сохранён для команды'
    } catch { /* shared state exposes the precise API error */ }
  }

  function addLogo(sourceId: string): void {
    const source = imageSources.find((candidate) => candidate.id === sourceId)
    if (!source) return
    addLibraryEntryToComposition(source)
    message = `Логотип «${source.title || source.filename}» добавлен на таймлайн`
  }
</script>

<details class="brand-kit-panel composition-tool-section">
  <summary>Бренд-кит · {spaceBrandState.kit.colors.length} цветов</summary>
  <div class="composition-tool-body">
    {#if authState.token && spacesState.selectedId}
      <p>Цвета и разрешённые export-шрифты становятся быстрыми пресетами в инспекторе текста. Логотипы ограничены image-источниками выбранного Space.</p>
      <div class="brand-colors">
        {#each colors as color, index (`${index}-${color.name}`)}
          <label>Название<input maxlength="64" value={color.name} oninput={(event) => updateColor(index, { name: event.currentTarget.value })} /></label>
          <label>Цвет<input aria-label={`Значение ${color.name}`} type="color" value={color.value.slice(0, 7)} oninput={(event) => updateColor(index, { value: event.currentTarget.value })} /></label>
          <button class="btn ghost sm danger" type="button" onclick={() => { colors = colors.filter((_, candidate) => candidate !== index) }}>Удалить</button>
        {/each}
      </div>
      <button class="btn ghost sm" type="button" disabled={colors.length >= 32} onclick={addColor}>+ Цвет бренда</button>
      <fieldset><legend>Шрифты бренда</legend>
        {#each FONTS as font (font)}<label class="brand-check"><input type="checkbox" checked={fonts.includes(font)} onchange={(event) => toggleFont(font, event.currentTarget.checked)} />{font}</label>{/each}
      </fieldset>
      <fieldset><legend>Логотипы</legend>
        {#each imageSources as source (source.id)}<span class="brand-logo-row"><label class="brand-check"><input type="checkbox" checked={logoSourceIds.includes(source.id)} onchange={(event) => toggleLogo(source.id, event.currentTarget.checked)} />{source.title || source.filename}</label>{#if spaceBrandState.kit.logoSourceIds.includes(source.id)}<button class="btn ghost sm" type="button" onclick={() => addLogo(source.id)}>На таймлайн</button>{/if}</span>
        {:else}<p>Сначала добавьте image-файл в выбранный Space-проект.</p>{/each}
      </fieldset>
      <button class="btn primary sm" type="button" disabled={spaceBrandState.loading} onclick={() => void save()}>Сохранить бренд-кит</button>
      {#if message}<p role="status">{message}</p>{/if}
      {#if spaceBrandState.error}<p class="error" role="alert">{spaceBrandState.error}</p>{/if}
    {:else}<p>Войдите и выберите Space.</p>{/if}
  </div>
</details>

<style>
  .brand-colors { display: grid; grid-template-columns: minmax(130px, 1fr) auto auto; gap: 7px; align-items: end; }
  .brand-colors label { display: grid; gap: 3px; }
  fieldset { display: flex; flex-wrap: wrap; gap: 8px 14px; border: 1px solid var(--border); border-radius: 8px; }
  .brand-check { display: inline-flex; align-items: center; gap: 5px; }
  .brand-logo-row { display: inline-flex; align-items: center; gap: 6px; }
  .error { color: var(--danger); }
</style>
