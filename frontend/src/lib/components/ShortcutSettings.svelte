<script lang="ts">
  import {
    SHORTCUT_DEFINITIONS,
    formatShortcutChord,
    keyboardEventToChord,
    shortcutDefinition,
    type ShortcutCommandId,
    type ShortcutGroup,
    type ShortcutMode,
  } from '$lib/shortcuts.js'
  import {
    beginShortcutCapture,
    cancelShortcutCapture,
    closeShortcutSettings,
    resetShortcutBindings,
    setShortcutBinding,
    shortcutState,
  } from '$lib/state/shortcuts.svelte.js'

  let dialog = $state<HTMLDialogElement>()
  let closeButton = $state<HTMLButtonElement>()
  let status = $state('')
  let localError = $state('')

  const modes: { id: ShortcutMode; label: string; description: string }[] = [
    { id: 'legacy', label: 'Legacy', description: 'Один исходник и упорядоченная монтажная линия' },
    { id: 'composition', label: 'Multitrack', description: 'Многодорожечная композиция' },
  ]
  const groupLabels: Record<ShortcutGroup, string> = {
    playback: 'Воспроизведение',
    history: 'История',
    timeline: 'Монтаж',
  }

  $effect(() => {
    const element = dialog
    if (!element) return
    if (shortcutState.dialogOpen && !element.open) {
      try {
        element.showModal()
      } catch {
        element.setAttribute('open', '')
      }
      queueMicrotask(() => closeButton?.focus())
    } else if (!shortcutState.dialogOpen && element.open) {
      element.close()
    }
  })

  function definitionsFor(mode: ShortcutMode) {
    return SHORTCUT_DEFINITIONS.filter((definition) => definition.mode === mode)
  }

  function begin(commandId: ShortcutCommandId): void {
    localError = ''
    status = `Ожидаю новое сочетание для «${shortcutDefinition(commandId).label}»`
    beginShortcutCapture(commandId)
  }

  function capture(event: KeyboardEvent, commandId: ShortcutCommandId): void {
    if (shortcutState.capturingCommandId !== commandId) return
    event.preventDefault()
    event.stopPropagation()
    if (event.key === 'Escape') {
      cancelShortcutCapture()
      status = 'Изменение отменено'
      return
    }
    if (event.key === 'Tab' || event.key === 'Enter') {
      localError = 'Tab и Enter зарезервированы для навигации по диалогу'
      return
    }
    const chord = keyboardEventToChord(event)
    if (!chord) {
      localError = 'Нажмите основную клавишу, при необходимости вместе с модификаторами'
      return
    }
    const result = setShortcutBinding(commandId, chord)
    if (result.ok) {
      localError = ''
      status = `Сочетание для «${shortcutDefinition(commandId).label}» сохранено`
    }
  }

  function clear(commandId: ShortcutCommandId): void {
    localError = ''
    const result = setShortcutBinding(commandId, null)
    if (result.ok) status = `Сочетание для «${shortcutDefinition(commandId).label}» отключено`
  }

  function reset(): void {
    try {
      resetShortcutBindings()
      localError = ''
      status = 'Сочетания по умолчанию восстановлены'
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error)
    }
  }

  function close(): void {
    status = ''
    localError = ''
    closeShortcutSettings()
  }

  function onCancel(event: Event): void {
    event.preventDefault()
    if (shortcutState.capturingCommandId) {
      cancelShortcutCapture()
      status = 'Изменение отменено'
    } else close()
  }

  function onDialogClick(event: MouseEvent): void {
    if (event.target === dialog) close()
  }

  function commandId(value: ShortcutCommandId): string {
    return `shortcut-${value.replace('.', '-')}`
  }
</script>

<dialog
  bind:this={dialog}
  class="shortcut-dialog"
  aria-labelledby="shortcut-dialog-title"
  aria-describedby="shortcut-dialog-description"
  aria-modal="true"
  oncancel={onCancel}
  onclose={closeShortcutSettings}
  onclick={onDialogClick}
>
  <div class="shortcut-dialog-panel">
    <header class="shortcut-dialog-head">
      <div>
        <h2 id="shortcut-dialog-title">Сочетания клавиш</h2>
        <p id="shortcut-dialog-description">Выберите команду и нажмите новое сочетание. Mod означает ⌘ на macOS и Ctrl на остальных платформах.</p>
      </div>
      <button bind:this={closeButton} class="btn ghost sm" type="button" aria-label="Закрыть настройки сочетаний" onclick={close}>Закрыть</button>
    </header>

    {#if localError || shortcutState.error}
      <p class="shortcut-message error" role="alert">{localError || shortcutState.error}</p>
    {/if}
    <p class="shortcut-message" role="status" aria-live="polite">{status}</p>

    <div class="shortcut-mode-list">
      {#each modes as mode (mode.id)}
        <section class="shortcut-mode" aria-labelledby={`shortcut-mode-${mode.id}`}>
          <div class="shortcut-mode-head">
            <h3 id={`shortcut-mode-${mode.id}`}>{mode.label}</h3>
            <p>{mode.description}</p>
          </div>
          <div class="shortcut-table-wrap">
            <table class="shortcut-table">
              <caption class="visually-hidden">Сочетания режима {mode.label}</caption>
              <thead><tr><th scope="col">Раздел</th><th scope="col">Команда</th><th scope="col">Сочетание</th><th scope="col"><span class="visually-hidden">Действия</span></th></tr></thead>
              <tbody>
                {#each definitionsFor(mode.id) as definition (definition.id)}
                  <tr>
                    <td>{groupLabels[definition.group]}</td>
                    <th id={commandId(definition.id)} scope="row"><span>{definition.label}</span><small>{definition.description}</small></th>
                    <td>
                      <button
                        class:capturing={shortcutState.capturingCommandId === definition.id}
                        class="shortcut-capture"
                        type="button"
                        aria-label={shortcutState.capturingCommandId === definition.id
                          ? `Введите новое сочетание для «${definition.label}»`
                          : `Изменить сочетание «${definition.label}», сейчас ${formatShortcutChord(shortcutState.bindings[definition.id])}`}
                        aria-pressed={shortcutState.capturingCommandId === definition.id}
                        onclick={() => begin(definition.id)}
                        onkeydown={(event) => capture(event, definition.id)}
                      >
                        {#if shortcutState.capturingCommandId === definition.id}
                          Нажмите клавиши…
                        {:else}
                          <kbd>{formatShortcutChord(shortcutState.bindings[definition.id])}</kbd>
                        {/if}
                      </button>
                    </td>
                    <td><button class="shortcut-clear" type="button" disabled={!shortcutState.bindings[definition.id]} aria-label={`Отключить сочетание «${definition.label}»`} onclick={() => clear(definition.id)}>×</button></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>
      {/each}
    </div>

    <footer class="shortcut-dialog-actions">
      <p>Изменения сохраняются локально сразу после назначения.</p>
      <button class="btn ghost sm" type="button" onclick={reset}>Сбросить по умолчанию</button>
      <button class="btn primary sm" type="button" onclick={close}>Готово</button>
    </footer>
  </div>
</dialog>
