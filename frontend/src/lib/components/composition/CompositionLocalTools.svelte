<script lang="ts">
  import {
    MAX_COMPOSITION_TEMPLATE_BYTES,
    type CompositionTemplate,
    type CompositionTemplateSlot,
    type TemplateReplacement,
  } from '$lib/composition/templates.js'
  import type { CompositionSource } from '$lib/composition/types.js'
  import {
    createSpaceTemplate,
    deleteSpaceTemplate,
    getSpaceTemplates,
    type SpaceTemplateDto,
  } from '$lib/api.js'
  import { MAX_SRT_BYTES } from '$lib/subtitles/srt.js'
  import {
    compositionSourceFromLibraryEntry,
    compositionState,
    applyCompositionTrackedAnimation,
    exportSelectedTextTrackSrt,
    importSrtToComposition,
    relinkCompositionSource,
    replaceCompositionAutoBeatMarkers,
    syncCompositionLibrary,
  } from '$lib/state/composition.svelte.js'
  import {
    compositionTemplateState,
    createTemplateFromCurrentComposition,
    deleteTemplateFromCatalog,
    exportTemplateFromCatalog,
    findTemplateClip,
    importTemplateToCatalog,
    instantiateCatalogTemplate,
    saveTemplateToCatalog,
    sourceCanReplaceTemplateSlot,
  } from '$lib/state/compositionTemplates.svelte.js'
  import { state as legacyState } from '$lib/state/store.svelte.js'
  import { authState } from '$lib/state/auth.svelte.js'
  import { spacesState } from '$lib/state/spaces.svelte.js'
  import MulticamPanel from './MulticamPanel.svelte'
  import CompositionRelinkPanel from './CompositionRelinkPanel.svelte'
  import CompositionTrackerPanel from './CompositionTrackerPanel.svelte'
  import AutoBeatPanel from './AutoBeatPanel.svelte'
  import BrandKitPanel from '$lib/components/collaboration/BrandKitPanel.svelte'
  import StockCatalogPanel from './StockCatalogPanel.svelte'

  let srtPicker = $state<HTMLInputElement>()
  let templatePicker = $state<HTMLInputElement>()
  let targetTextTrackId = $state('')
  let templateName = $state('')
  let replacements = $state<Record<string, string>>({})
  let localMessage = $state('')
  let localError = $state('')
  let teamTemplates = $state<SpaceTemplateDto[]>([])
  let teamBusy = $state(false)

  const selectedTemplate = $derived(
    compositionTemplateState.templates.find((template) => template.id === compositionTemplateState.selectedTemplateId) ?? null,
  )
  const catalogBytes = $derived(new TextEncoder().encode(JSON.stringify(compositionTemplateState.templates)).byteLength)
  const librarySources = $derived.by(() => legacyState.library.flatMap((entry) => {
    if (entry.kind !== 'source') return []
    try {
      return [{ entry, source: compositionSourceFromLibraryEntry(entry) }]
    } catch {
      return []
    }
  }))
  const missingSourceIds = $derived(
    Object.keys(compositionState.document.sources)
      .filter((sourceId) => !compositionState.media[sourceId])
      .sort(),
  )
  const relinkCandidates = $derived(librarySources.map(({ entry, source }) => ({
    source,
    label: entry.title?.trim() || entry.filename,
  })))

  $effect(() => {
    const token = authState.token
    const spaceId = spacesState.selectedId
    if (token && spaceId) void loadTeamTemplates(spaceId, token)
    else teamTemplates = []
  })

  async function loadTeamTemplates(spaceId: string, token: string): Promise<void> {
    try {
      teamTemplates = await getSpaceTemplates(spaceId, token)
    } catch (error) {
      showError(error)
    }
  }

  async function publishSelectedTemplate(): Promise<void> {
    const token = authState.token
    const spaceId = spacesState.selectedId
    const template = selectedTemplate
    if (!token || !spaceId || !template || teamBusy) return
    teamBusy = true
    try {
      const published = await createSpaceTemplate(spaceId, token, template)
      teamTemplates = [published, ...teamTemplates]
      showSuccess(`Шаблон «${template.name}» опубликован для команды`)
    } catch (error) {
      showError(error)
    } finally {
      teamBusy = false
    }
  }

  function addTeamTemplateToCatalog(shared: SpaceTemplateDto): void {
    try {
      saveTemplateToCatalog(shared.template)
      compositionTemplateState.selectedTemplateId = shared.template.id
      showSuccess(`Командный шаблон «${shared.template.name}» добавлен локально`)
    } catch (error) {
      showError(error)
    }
  }

  async function removeTeamTemplate(shared: SpaceTemplateDto): Promise<void> {
    const token = authState.token
    const spaceId = spacesState.selectedId
    if (!token || !spaceId || teamBusy) return
    if (typeof window !== 'undefined' && !window.confirm(`Удалить командный шаблон «${shared.template.name}»?`)) return
    teamBusy = true
    try {
      await deleteSpaceTemplate(spaceId, shared.id, token)
      teamTemplates = teamTemplates.filter((candidate) => candidate.id !== shared.id)
      showSuccess('Командный шаблон удалён')
    } catch (error) {
      showError(error)
    } finally {
      teamBusy = false
    }
  }

  $effect(() => {
    const template = selectedTemplate
    const candidates = librarySources
    if (!template) {
      replacements = {}
      return
    }
    const defaults: Record<string, string> = {}
    for (const slot of template.slots) {
      const clip = findTemplateClip(template, slot.clipId)
      if (slot.kind === 'text') defaults[slot.id] = clip?.kind === 'text' ? clip.text : ''
      else {
        const originalId = clip && clip.kind !== 'text' ? clip.sourceId : ''
        defaults[slot.id] = candidates.some(
          (candidate) => candidate.source.id === originalId && sourceCanReplaceTemplateSlot(template, slot, candidate.source),
        )
          ? originalId
          : candidates.find((candidate) => sourceCanReplaceTemplateSlot(template, slot, candidate.source))?.source.id ?? ''
      }
    }
    replacements = defaults
  })

  function showSuccess(message: string): void {
    localError = ''
    localMessage = message
  }

  function showError(error: unknown): void {
    localMessage = ''
    localError = error instanceof Error ? error.message : String(error)
  }

  async function importSrtFile(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    try {
      if (file.size > MAX_SRT_BYTES) throw new Error('SRT превышает лимит 2 МиБ')
      const count = importSrtToComposition(await file.text(), targetTextTrackId || undefined)
      showSuccess(`Импортировано субтитров: ${count}`)
    } catch (error) {
      showError(error)
    }
  }

  function exportSrt(): void {
    try {
      const result = exportSelectedTextTrackSrt()
      downloadText(result.filename, result.text, 'application/x-subrip;charset=utf-8')
      showSuccess(`SRT сохранён локально: ${result.filename}`)
    } catch (error) {
      showError(error)
    }
  }

  function createTemplate(): void {
    try {
      const name = templateName.trim() || `${compositionState.projectName} — шаблон`
      const template = createTemplateFromCurrentComposition(name)
      templateName = ''
      showSuccess(`Создан шаблон с ${template.slots.length} слотами`)
    } catch (error) {
      showError(error)
    }
  }

  async function importTemplateFile(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    try {
      if (file.size > MAX_COMPOSITION_TEMPLATE_BYTES) throw new Error('Шаблон превышает лимит 2 МиБ')
      const template = importTemplateToCatalog(await file.text())
      showSuccess(`Импортирован шаблон «${template.name}»`)
    } catch (error) {
      showError(error)
    }
  }

  function exportTemplate(): void {
    const id = compositionTemplateState.selectedTemplateId
    if (!id) return
    try {
      const result = exportTemplateFromCatalog(id)
      downloadText(result.filename, result.text, 'application/json;charset=utf-8')
      showSuccess(`Шаблон сохранён локально: ${result.filename}`)
    } catch (error) {
      showError(error)
    }
  }

  function instantiateTemplate(template: CompositionTemplate): void {
    try {
      const values: Record<string, TemplateReplacement> = {}
      for (const slot of template.slots) {
        const value = replacements[slot.id] ?? ''
        if (slot.kind === 'text') values[slot.id] = { text: value }
        else {
          const source = librarySources.find((candidate) => candidate.source.id === value)?.source
          if (!source) throw new Error(`Выберите медиа для слота «${slot.label}»`)
          values[slot.id] = { source }
        }
      }
      instantiateCatalogTemplate(template.id, values)
      syncCompositionLibrary(legacyState.library, legacyState.librarySnapshotReady)
      showSuccess(`Шаблон «${template.name}» создан как новая композиция`)
    } catch (error) {
      showError(error)
    }
  }

  function removeTemplate(template: CompositionTemplate): void {
    if (typeof window !== 'undefined' && !window.confirm(`Удалить шаблон «${template.name}»?`)) return
    try {
      deleteTemplateFromCatalog(template.id)
      showSuccess('Шаблон удалён')
    } catch (error) {
      showError(error)
    }
  }

  function candidatesFor(template: CompositionTemplate, slot: CompositionTemplateSlot) {
    return librarySources.filter((candidate) => sourceCanReplaceTemplateSlot(template, slot, candidate.source))
  }

  function sourceLabel(source: CompositionSource): string {
    const entry = librarySources.find((candidate) => candidate.source.id === source.id)?.entry
    return entry?.title?.trim() || entry?.filename || source.id
  }

  function relinkMissingSource(sourceId: string, replacement: CompositionSource): void {
    const entry = librarySources.find((candidate) => candidate.source.id === replacement.id)?.entry
    relinkCompositionSource(sourceId, entry ?? replacement)
    showSuccess(`Источник ${sourceId} перепривязан к ${replacement.id}`)
  }

  function downloadText(filename: string, text: string, type: string): void {
    const url = URL.createObjectURL(new Blob([text], { type }))
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = filename
    anchor.hidden = true
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }
</script>

<section class="composition-local-tools card" aria-label="Локальные инструменты композиции">
  <CompositionRelinkPanel
    document={compositionState.document}
    {missingSourceIds}
    candidates={relinkCandidates}
    onrelink={relinkMissingSource}
  />
  <CompositionTrackerPanel
    document={compositionState.document}
    media={compositionState.media}
    onapply={applyCompositionTrackedAnimation}
  />
  <AutoBeatPanel
    document={compositionState.document}
    media={compositionState.media}
    onapply={(beats, bpm) => { replaceCompositionAutoBeatMarkers(beats, bpm) }}
  />
  <MulticamPanel />
  <BrandKitPanel />
  <StockCatalogPanel />
  <details class="composition-tool-section">
    <summary>Субтитры SRT / TXT</summary>
    <div class="composition-tool-body">
      <p>Импорт SRT и TXT с таймкодами выполняется локально в UTF-8, без сервера и распознавания речи.</p>
      <label>
        Дорожка для импорта
        <select bind:value={targetTextTrackId}>
          <option value="">Выбранная text-дорожка или новая</option>
          {#each compositionState.document.tracks.filter((track) => track.kind === 'text') as track (track.id)}
            <option value={track.id}>{track.name}{track.locked ? ' · locked' : ''}</option>
          {/each}
        </select>
      </label>
      <div class="composition-tool-actions">
        <button class="btn ghost sm" type="button" onclick={() => srtPicker?.click()}>Импорт SRT / TXT</button>
        <input bind:this={srtPicker} class="hidden-file" type="file" accept=".srt,.txt,application/x-subrip,text/plain" onchange={(event) => void importSrtFile(event)} />
        <button class="btn ghost sm" type="button" onclick={exportSrt}>Экспорт выбранной text-дорожки</button>
      </div>
    </div>
  </details>

  <details class="composition-tool-section">
    <summary>Локальные шаблоны</summary>
    <div class="composition-tool-body">
      <p>JSON-пакеты детерминированы и не содержат путей к файлам. Каталог: {compositionTemplateState.templates.length}/32 · {Math.ceil(catalogBytes / 1024)} КиБ.</p>
      <div class="composition-template-create">
        <input aria-label="Название нового шаблона" placeholder={`${compositionState.projectName} — шаблон`} bind:value={templateName} maxlength="128" />
        <button class="btn ghost sm" type="button" onclick={createTemplate}>Создать из композиции</button>
        <button class="btn ghost sm" type="button" onclick={() => templatePicker?.click()}>Импорт JSON</button>
        <input bind:this={templatePicker} class="hidden-file" type="file" accept=".json,application/json" onchange={(event) => void importTemplateFile(event)} />
      </div>

      {#if compositionTemplateState.templates.length}
        <label>
          Шаблон
          <select bind:value={compositionTemplateState.selectedTemplateId}>
            {#each compositionTemplateState.templates as template (template.id)}
              <option value={template.id}>{template.name} · {template.slots.length} слотов</option>
            {/each}
          </select>
        </label>
      {/if}

      {#if selectedTemplate}
        <div class="composition-template-slots">
          {#each selectedTemplate.slots as slot (slot.id)}
            {#if slot.kind === 'text'}
              <label>
                {slot.label}
                <input maxlength="512" bind:value={replacements[slot.id]} />
              </label>
            {:else}
              {@const candidates = candidatesFor(selectedTemplate, slot)}
              <label>
                {slot.label}
                <select bind:value={replacements[slot.id]} class:invalid={!replacements[slot.id]}>
                  <option value="">Выберите совместимое медиа…</option>
                  {#each candidates as candidate (candidate.source.id)}
                    <option value={candidate.source.id}>{sourceLabel(candidate.source)} · {candidate.source.kind}</option>
                  {/each}
                </select>
                {#if !candidates.length}<small>В медиатеке нет совместимого источника достаточной длительности.</small>{/if}
              </label>
            {/if}
          {:else}
            <p>У этого шаблона нет слотов; будет создана точная копия документа.</p>
          {/each}
        </div>
        <div class="composition-tool-actions">
          <button class="btn primary sm" type="button" onclick={() => instantiateTemplate(selectedTemplate)}>Создать композицию</button>
          <button class="btn ghost sm" type="button" onclick={exportTemplate}>Экспорт JSON</button>
          <button class="btn ghost sm danger" type="button" onclick={() => removeTemplate(selectedTemplate)}>Удалить</button>
        </div>
      {:else}
        <p>Создайте шаблон из текущей композиции или импортируйте JSON-пакет.</p>
      {/if}
    </div>
  </details>

  <details class="composition-tool-section">
    <summary>Командные шаблоны · {teamTemplates.length}</summary>
    <div class="composition-tool-body">
      {#if authState.token && spacesState.selectedId}
        <p>Шаблоны выбранного Space доступны всем участникам; публиковать и удалять могут owner/editor.</p>
        <div class="composition-tool-actions">
          <button
            class="btn ghost sm"
            type="button"
            disabled={!selectedTemplate || teamBusy}
            onclick={() => void publishSelectedTemplate()}
          >Опубликовать выбранный локальный</button>
          <button
            class="btn ghost sm"
            type="button"
            disabled={teamBusy}
            onclick={() => void loadTeamTemplates(spacesState.selectedId, authState.token ?? '')}
          >Обновить</button>
        </div>
        {#if teamTemplates.length}
          <ul class="composition-team-templates">
            {#each teamTemplates as shared (shared.id)}
              <li>
                <span><strong>{shared.template.name}</strong><small>{shared.template.slots.length} слотов · {shared.createdBy} · r{shared.revision}</small></span>
                <span class="composition-tool-actions">
                  <button class="btn ghost sm" type="button" onclick={() => addTeamTemplateToCatalog(shared)}>Добавить локально</button>
                  <button class="btn ghost sm danger" type="button" disabled={teamBusy} onclick={() => void removeTeamTemplate(shared)}>Удалить</button>
                </span>
              </li>
            {/each}
          </ul>
        {:else}
          <p>В этом пространстве пока нет опубликованных шаблонов.</p>
        {/if}
      {:else}
        <p>Войдите и выберите Space, чтобы использовать командный каталог.</p>
      {/if}
    </div>
  </details>

  {#if localMessage}<p class="composition-local-message" role="status">{localMessage}</p>{/if}
  {#if localError}<p class="error" role="alert">{localError}</p>{/if}
</section>
