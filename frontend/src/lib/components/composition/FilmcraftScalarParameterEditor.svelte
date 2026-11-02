<script lang="ts">
  import { editFilmcraftAnimation, setFilmcraftEffectParameter, applyFilmcraftEffect, listFilmcraftEffects, type FilmcraftEffectOption, editFilmcraftEffectRack, type FilmcraftEffectRackCommand, type FilmcraftSourceEditFence, getFilmcraftParameters, editFilmcraftParameter, editFilmcraftMask, type FilmcraftMaskCommand, type FilmcraftParameterReport, type FilmcraftParameterCommand, type FilmcraftParameterValue, type FilmcraftImportResult } from '$lib/api'
  import FilmcraftMaskMonitor from './FilmcraftMaskMonitor.svelte'
  import FilmcraftMaskTrackingControls from './FilmcraftMaskTrackingControls.svelte'
  import FilmcraftMaskControls from './FilmcraftMaskControls.svelte'
  import FilmcraftPathValueEditor from './FilmcraftPathValueEditor.svelte'
  let { document, sequenceId, clipId, timeFrames, timeTicks, token, trackKind = 'Video', sources = {}, disabled = false, onedit }: { document: string; sequenceId: string; clipId: string; timeFrames: string; timeTicks?: string; token?: string | null; trackKind?: 'Video' | 'Audio'; sources?: Record<string, string>; disabled?: boolean; onedit: (project: FilmcraftImportResult, expected?: FilmcraftSourceEditFence) => boolean | void } = $props()
  let open = $state(false)
  let report = $state<FilmcraftParameterReport | null>(null)
  let effectIndex = $state(0)
  let parameterName = $state('')
  let maskIndex = $state(-1)
  let busy = $state(false)
  let loading = $state(false)
  let acceptedDocument: string | undefined
  function commitProject(project: FilmcraftImportResult, expected?: FilmcraftSourceEditFence) { const accepted = expected ? onedit(project, expected) : onedit(project); if (accepted !== false) acceptedDocument = project.documentText; return accepted }
  let trackingBusy = $state(false)
  let monitorBusy = $state(false)
  let error = $state('')
  let revision = $state(0)
  let keyTime = $state('0')
  let destination = $state('0')
  let scalarText = $state('1')
  let componentTexts = $state<string[]>([])
  let draftOriginal = $state<FilmcraftParameterValue>({ Float: 1 })
  const componentNames = $derived('Vec2' in draftOriginal ? ['x', 'y'] : 'Color' in draftOriginal ? ['R', 'G', 'B', 'A'] : [])
  let interpolation = $state('Linear')
  let incoming = $state('0.3333333333333333')
  let outgoing = $state('0.3333333333333333')
  const identity = $derived(JSON.stringify([document, sequenceId, clipId, timeFrames, timeTicks, token, sources, disabled, trackKind]))
  const selectedEffect = $derived(report?.effects.find(e => e.index === effectIndex))
  const parameters = $derived(maskIndex < 0 ? selectedEffect?.parameters : selectedEffect?.masks?.find(m => m.index === maskIndex)?.parameters)
  const parameter = $derived(parameters?.find(p => p.name === parameterName))
  const clock = $derived(timeTicks ? { timeTicks } : { timeFrames })
  let choosingEffect = $state(false)
  let catalog = $state<FilmcraftEffectOption[]>([])
  let catalogLoading = $state(false)
  let catalogError = $state('')
  let catalogRevision = $state(0)
  let effectQuery = $state('')
  let newEffect = $state('')
  // Intrinsic and graphic layers have dedicated authoring controls in the pinned source.
  const dedicatedEffects = ['motion', 'opacity', 'time_remap', 'volume', 'channel_volume', 'panner', 'graphic_text', 'graphic_shape']
  const visibleEffects = $derived(catalog.filter(effect => `${effect.name} ${effect.path}`.toLowerCase().includes(effectQuery.toLowerCase())))
  $effect(() => { if (!visibleEffects.some(effect => effect.id === newEffect)) newEffect = visibleEffects[0]?.id ?? '' })
  $effect(() => {
    const actor = token; const kind = trackKind; const retry = catalogRevision
    void retry
    if (!open || !choosingEffect || !actor || disabled) { catalog = []; catalogLoading = false; return }
    const controller = new AbortController()
    catalogLoading = true; catalogError = ''; catalog = []
    listFilmcraftEffects(actor, kind, controller.signal).then(effects => {
      if (controller.signal.aborted || actor !== token || kind !== trackKind) return
      catalog = effects.filter(effect => effect.kind === kind && !dedicatedEffects.includes(effect.id))
      catalogLoading = false
    }).catch(reason => { if (!controller.signal.aborted && actor === token && kind === trackKind) { catalogError = String(reason); catalogLoading = false } })
    return () => controller.abort()
  })
  let editController: AbortController | undefined
  $effect(() => {
    const owner = identity
    const reload = revision
    if (!open || disabled) { report = null; loading = false; return }
    void reload
    const controller = new AbortController()
    loading = true
    if (acceptedDocument !== document) report = null
    acceptedDocument = undefined
    error = ''
    getFilmcraftParameters({ document, sequenceId, clipId, ...clock }, controller.signal).then(value => {
      if (controller.signal.aborted || owner !== identity) return
      report = value
      loading = false
      if (!value.effects.some(e => e.index === effectIndex)) effectIndex = value.effects[0]?.index ?? 0
      const selected = value.effects.find(e => e.index === effectIndex)
      if (maskIndex >= 0 && !selected?.masks?.some(m => m.index === maskIndex)) maskIndex = -1
      const params = maskIndex < 0 ? selected?.parameters : selected?.masks?.find(m => m.index === maskIndex)?.parameters
      if (!params?.some(p => p.name === parameterName)) parameterName = params?.[0]?.name ?? ''
    }).catch(reason => { if (!controller.signal.aborted && owner === identity) { error = String(reason); report = null; loading = false } })
    return () => { controller.abort(); editController?.abort() }
  })
  function loadValue(value: FilmcraftParameterValue) {
    draftOriginal = JSON.parse(JSON.stringify(value)) as FilmcraftParameterValue
    if ('Vec2' in value) componentTexts = [value.Vec2.x, value.Vec2.y].map(n => n === null ? 'auto' : String(n))
    else if ('Color' in value) componentTexts = value.Color.map(String)
    else if ('Text' in value) { scalarText = value.Text; componentTexts = [] }
    else if ('Curve' in value || 'Path' in value) componentTexts = []
    else { scalarText = String(Object.values(value)[0]); componentTexts = [] }
  }
  $effect(() => { const current = parameter; if (current) loadValue(current.value) })
  function selectKey(time: string) {
    const key = parameter?.keys.find(k => k.time === time)
    if (!key) return
    keyTime = key.time
    loadValue(key.value)
    interpolation = key.interp
    incoming = String(key.in_influence ?? 1 / 3)
    outgoing = String(key.out_influence ?? 1 / 3)
  }
  function value(): FilmcraftParameterValue {
    const original = draftOriginal
    if ('Text' in original) {
      if (new TextEncoder().encode(scalarText).length > 65536) throw new Error('Текст превышает 65536 байт')
      return { Text: scalarText }
    }
    if ('Curve' in original) {
      if (original.Curve.length > 4096 || original.Curve.some(pair => pair.length !== 2 || pair.some(n => !Number.isFinite(n) || !Number.isFinite(Math.fround(n))))) throw new Error('Кривая: не более 4096 конечных пар f32')
      return JSON.parse(JSON.stringify(original)) as FilmcraftParameterValue
    }
    if ('Path' in original) {
      if (original.Path.vertices.length > 4096 || original.Path.vertices.some(vertex => [vertex.p, vertex.t_in, vertex.t_out].some((point, index) => !point ? index === 0 : !Number.isFinite(point.x) || !Number.isFinite(point.y)))) throw new Error('Путь: не более 4096 вершин с конечными координатами')
      return JSON.parse(JSON.stringify(original)) as FilmcraftParameterValue
    }
    if ('Vec2' in original) {
      const coordinate = (text: string) => {
        if (text.trim() === 'auto') return null
        const n = Number(text)
        if (!text.trim() || !Number.isFinite(n)) throw new Error('Координата: конечное число или auto')
        return n
      }
      return { Vec2: { x: coordinate(componentTexts[0] ?? ''), y: coordinate(componentTexts[1] ?? '') } }
    }
    if ('Color' in original) {
      const color = componentTexts.map(text => {
        const n = Number(text)
        if (!text.trim() || !Number.isFinite(Math.fround(n))) throw new Error('Канал RGBA: конечное число f32')
        return n
      })
      if (color.length !== 4) throw new Error('Требуются четыре канала RGBA')
      return { Color: color as [number, number, number, number] }
    }
    if (original && 'Bool' in original) {
      if (!['true', 'false'].includes(scalarText)) throw new Error('Введите true или false')
      return { Bool: scalarText === 'true' }
    }
    const number = Number(scalarText)
    if (!scalarText.trim() || !Number.isFinite(number)) throw new Error('Требуется конечное число')
    if (original && 'Choice' in original) {
      if (!Number.isInteger(number) || number < 0 || number > 4294967295) throw new Error('Choice: целое 0…4294967295')
      return { Choice: number }
    }
    return { Float: number }
  }
  function exactTime(text: string) {
    if (!/^-?\d+$/.test(text) || BigInt(text) < -9223372036854775808n || BigInt(text) > 9223372036854775807n) throw new Error('Время должно быть signed i64')
    return text
  }
  async function addEffect() {
    if (!token || disabled || loading || busy || trackingBusy || monitorBusy || report?.locked || !report || catalogLoading || !visibleEffects.some(effect => effect.id === newEffect)) return
    const owner = identity
    const chosen = newEffect
    const fence = { document, sequenceId, clipId, token: token ?? null }
    const controller = new AbortController()
    editController = controller; busy = true; error = ''
    try {
      const changed = await applyFilmcraftEffect(token, { document, sequence: sequenceId, clip: clipId, effect: chosen }, controller.signal)
      if (!controller.signal.aborted && owner === identity && open && choosingEffect && chosen === newEffect) {
        maskIndex = -1; parameterName = ''; choosingEffect = false; effectQuery = ''
        acceptedDocument = changed.documentText
        onedit(changed, fence)
      }
    } catch (reason) { if (!controller.signal.aborted && owner === identity) error = String(reason) }
    finally { if (editController === controller) { busy = false; editController = undefined } }
  }
  async function applyRack(command: FilmcraftEffectRackCommand) {
    if (!token || disabled || loading || busy || trackingBusy || monitorBusy || report?.locked || !selectedEffect) return
    const owner = identity
    const baseDocument = document
    const fence = { document: baseDocument, sequenceId, clipId, token: token ?? null }
    const selection = `${selectedEffect.index}:${selectedEffect.kind}`
    const controller = new AbortController()
    editController = controller; busy = true; error = ''
    try {
      const changed = await editFilmcraftEffectRack(token, { document: baseDocument, sequence: sequenceId, clip: clipId, index: selectedEffect.index, command }, controller.signal)
      if (!controller.signal.aborted && owner === identity && open && selection === `${selectedEffect?.index}:${selectedEffect?.kind}`) {
        if (command !== 'effects.toggleEnabled') { maskIndex = -1; parameterName = '' }
        acceptedDocument = changed.documentText
        onedit(changed, fence)
      }
    } catch (reason) { if (!controller.signal.aborted && owner === identity) error = String(reason) }
    finally { if (editController === controller) { busy = false; editController = undefined } }
  }
  async function applyMask(command: FilmcraftMaskCommand) {
    if (disabled || loading || busy || trackingBusy || monitorBusy || report?.locked || !selectedEffect) return
    const owner = identity
    const selection = `${selectedEffect.index}:${maskIndex}:${parameterName}`
    const controller = new AbortController()
    editController = controller
    busy = true
    error = ''
    try {
      const changed = await editFilmcraftMask({ document, address: { sequenceId, clipId, effectIndex: selectedEffect.index, effectKind: selectedEffect.kind }, command }, controller.signal)
      if (!controller.signal.aborted && owner === identity && open && selection === `${selectedEffect?.index}:${maskIndex}:${parameterName}`) {
        if (command.kind === 'create') { maskIndex = selectedEffect.masks?.length ?? 0; parameterName = 'path' }
        else if (command.kind === 'delete') { maskIndex = -1; parameterName = selectedEffect.parameters[0]?.name ?? '' }
        else if (command.kind === 'move') maskIndex = command.destination
        commitProject(changed)
      }
    } catch (reason) { if (!controller.signal.aborted && owner === identity) error = String(reason) }
    finally { if (editController === controller) { busy = false; editController = undefined } }
  }
  async function applySourceValue(command: 'effects.setParam' | 'effects.toggleAnimation' | 'effects.addKeyframe' = 'effects.setParam') {
    if (!token || disabled || loading || busy || trackingBusy || monitorBusy || report?.locked || !report || !selectedEffect || !parameter) return
    const owner = identity
    const fence = { document, sequenceId, clipId, token: token ?? null }
    const selection = `${selectedEffect.index}:${maskIndex}:${parameter.name}`
    const controller = new AbortController()
    editController = controller; busy = true; error = ''
    try {
      const changed = command !== 'effects.setParam' ? await editFilmcraftAnimation(token, { command, document, sequence: sequenceId, clip: clipId, effect: selectedEffect.index, param: parameter.name, ...(maskIndex < 0 ? {} : { mask: maskIndex }), playhead: report.timelineTicks }, controller.signal) : await setFilmcraftEffectParameter(token, { document, sequence: sequenceId, clip: clipId, effect: selectedEffect.index, param: parameter.name, value: value(), ...(maskIndex < 0 ? {} : { mask: maskIndex }), ...clock }, controller.signal)
      if (!controller.signal.aborted && owner === identity && open && selection === `${selectedEffect?.index}:${maskIndex}:${parameter?.name}`) {
        acceptedDocument = changed.documentText
        onedit(changed, fence)
      }
    } catch (reason) { if (!controller.signal.aborted && owner === identity) error = String(reason) }
    finally { if (editController === controller) { busy = false; editController = undefined } }
  }
  async function apply(make: () => FilmcraftParameterCommand) {
    if (disabled || loading || busy || trackingBusy || monitorBusy || report?.locked || !selectedEffect || !parameter) return
    const owner = identity
    const selection = `${selectedEffect.index}:${maskIndex}:${parameter.name}`
    const controller = new AbortController()
    editController = controller
    busy = true
    error = ''
    try {
      const command = make()
      const changed = await editFilmcraftParameter({ document, address: { sequenceId, clipId, effectIndex: selectedEffect.index, effectKind: selectedEffect.kind, parameter: parameter.name, ...(maskIndex < 0 ? {} : { maskIndex }) }, command, ...clock }, controller.signal)
      if (!controller.signal.aborted && owner === identity && open && selection === `${selectedEffect?.index}:${maskIndex}:${parameter?.name}`) commitProject(changed)
    } catch (reason) { if (!controller.signal.aborted && owner === identity) error = String(reason) }
    finally { if (editController === controller) { busy = false; editController = undefined } }
  }
</script>

<details bind:open>
  <summary>Ключи параметров Filmcraft</summary>
  {#if error}<p role="alert">{error}</p><button onclick={() => revision++}>Повторить</button>{/if}
  {#if report}
    <p>Время параметров: {report.parameterTicks}; кадр: {report.frameTicks}. Тиков в секунду: 254016000000.</p>
    {#if report.locked}<p>Дорожка заблокирована</p>{/if}
    <fieldset disabled={disabled || loading || busy || trackingBusy || monitorBusy || report.locked}>
      <button disabled={!token} onclick={() => choosingEffect = !choosingEffect}>{choosingEffect ? 'Закрыть выбор эффекта' : 'Добавить эффект…'}</button>
      {#if choosingEffect}
        {#if catalogLoading}<span>Загрузка эффектов…</span>{/if}
        {#if catalogError}<p role="alert">{catalogError}</p><button onclick={() => catalogRevision++}>Повторить загрузку эффектов</button>{/if}
        <label>Поиск <input aria-label="Поиск эффекта Filmcraft" bind:value={effectQuery} /></label>
        <label>Новый эффект <select aria-label="Новый эффект Filmcraft" bind:value={newEffect} disabled={catalogLoading}>{#each visibleEffects as effect (effect.id)}<option value={effect.id}>{effect.path ? `${effect.path} / ` : ''}{effect.name}</option>{/each}</select></label>
        {#if !catalogLoading && !catalogError && !visibleEffects.length}<span>Подходящих эффектов нет</span>{/if}
        <button aria-label="Применить новый эффект Filmcraft" disabled={catalogLoading || !visibleEffects.some(effect => effect.id === newEffect)} onclick={addEffect}>Применить эффект</button>
      {/if}
      <label>Эффект <select aria-label="Эффект параметров Filmcraft" value={effectIndex} onchange={event => { effectIndex = Number(event.currentTarget.value); maskIndex = -1; parameterName = report?.effects.find(e => e.index === effectIndex)?.parameters[0]?.name ?? '' }}>{#each report.effects as selectedEffect (selectedEffect.index)}<option value={selectedEffect.index}>{selectedEffect.index + 1}: {selectedEffect.kind}</option>{/each}</select></label>
      {#if selectedEffect}
        <button aria-label="Переключить эффект Filmcraft" disabled={!token} onclick={() => applyRack('effects.toggleEnabled')}>{selectedEffect.enabled ? 'Выключить эффект' : 'Включить эффект'}</button>
        <button aria-label="Сбросить эффект Filmcraft" disabled={!token} onclick={() => applyRack('effects.reset')}>Сбросить эффект</button>
        <button aria-label="Удалить эффект Filmcraft" disabled={!token} onclick={() => applyRack('effects.remove')}>Удалить эффект</button>
      {/if}
      {#if selectedEffect}<FilmcraftMaskControls maskable={selectedEffect.maskable !== false} masks={selectedEffect.masks ?? []} {maskIndex} disabled={disabled || loading || busy || trackingBusy || monitorBusy || report.locked} oncommand={applyMask} />{/if}
      {#if selectedEffect?.masks?.length}<label>Объект параметров <select aria-label="Маска параметров Filmcraft" value={maskIndex} onchange={event => { maskIndex = Number(event.currentTarget.value); const params = maskIndex < 0 ? selectedEffect?.parameters : selectedEffect?.masks?.find(m => m.index === maskIndex)?.parameters; parameterName = params?.[0]?.name ?? '' }}><option value={-1}>Эффект</option>{#each selectedEffect.masks as mask (mask.index)}<option value={mask.index}>Маска {mask.index + 1}: {mask.name ?? ''}</option>{/each}</select></label>{/if}
      <label>Параметр <select aria-label="Параметр Filmcraft" bind:value={parameterName}>{#each parameters ?? [] as parameter (parameter.name)}<option value={parameter.name}>{parameter.name}</option>{/each}</select></label>
      {#if parameter}
        <p>Базовое значение: {JSON.stringify(parameter.value)}; в позиции курсора: {parameter.sampledComponents ? JSON.stringify(parameter.sampledComponents) : parameter.sampledValue ? JSON.stringify(parameter.sampledValue) : parameter.sampled ?? 'нет численного образца'}</p>
        <label>Ключ <select aria-label="Ключ параметра Filmcraft" onchange={event => selectKey(event.currentTarget.value)}><option value="">Выберите ключ</option>{#each parameter.keys as key (key.time)}<option value={key.time}>{key.time}: {JSON.stringify(key.value)}</option>{/each}</select></label>
        <label>Точное время, тиков <input aria-label="Точное время ключа Filmcraft" bind:value={keyTime} /></label>
        {#if 'Text' in draftOriginal}
          <label>Текст <textarea aria-label="Текст параметра Filmcraft" bind:value={scalarText}></textarea></label>
        {:else if 'Path' in draftOriginal}
          <FilmcraftPathValueEditor bind:path={draftOriginal.Path} />
        {:else if 'Curve' in draftOriginal}
          {#each draftOriginal.Curve as pair, index (index)}
            <label>X {index + 1} <input aria-label={`Точка кривой ${index + 1} X Filmcraft`} type="number" step="any" bind:value={pair[0]} /></label>
            <label>Y {index + 1} <input aria-label={`Точка кривой ${index + 1} Y Filmcraft`} type="number" step="any" bind:value={pair[1]} /></label>
            <button onclick={() => { if ('Curve' in draftOriginal) draftOriginal.Curve = draftOriginal.Curve.filter((_, i) => i !== index) }}>Удалить точку кривой {index + 1}</button>
          {/each}
          <button disabled={draftOriginal.Curve.length >= 4096} onclick={() => { if ('Curve' in draftOriginal) draftOriginal.Curve = [...draftOriginal.Curve, [0, 0]] }}>Добавить точку кривой Filmcraft</button>
        {:else if componentNames.length}
          {#each componentNames as name, index (name)}<label>{name} <input aria-label={`Компонент ${name} Filmcraft`} bind:value={componentTexts[index]} /></label>{/each}
          {#if 'Vec2' in draftOriginal}<p>auto сохраняет автоматическую координату; её образец требует геометрию слоя.</p>{/if}
        {:else}<label>Значение <input aria-label="Значение параметра Filmcraft" bind:value={scalarText} /></label>{/if}
        <button disabled={!token} onclick={() => applySourceValue('effects.toggleAnimation')}>{parameter.keys.length ? 'Отключить анимацию' : 'Включить анимацию'}</button>
        <button disabled={!token} onclick={() => applySourceValue('effects.addKeyframe')}>Добавить/удалить ключ в позиции курсора</button>
        <button disabled={!token} onclick={() => applySourceValue()}>Применить значение в позиции курсора</button>
        <button onclick={() => apply(() => ({ kind: 'setBase', value: value() }))}>Изменить базовое значение</button>
        <button onclick={() => apply(() => ({ kind: 'upsert', time: exactTime(keyTime), value: value() }))}>Сохранить ключ Filmcraft</button>
        <button onclick={() => apply(() => ({ kind: 'atPlayhead', value: value() }))}>Ключ Filmcraft в позиции курсора</button>
        <button onclick={() => apply(() => ({ kind: 'delete', time: exactTime(keyTime) }))}>Удалить ключ Filmcraft</button>
        <label>Новое время, тиков <input aria-label="Новое время ключа Filmcraft" bind:value={destination} /></label>
        <button onclick={() => apply(() => ({ kind: 'move', time: exactTime(keyTime), destination: exactTime(destination) }))}>Переместить ключ Filmcraft</button>
        <label>Интерполяция <select bind:value={interpolation}>{#each ['Linear', 'Hold', 'Bezier', 'AutoBezier', 'ContinuousBezier', 'EaseIn', 'EaseOut'] as mode (mode)}<option>{mode}</option>{/each}</select></label>
        <button onclick={() => apply(() => ({ kind: 'interpolation', time: exactTime(keyTime), interpolation }))}>Применить интерполяцию</button>
        <label>Входящее влияние <input bind:value={incoming} /></label><label>Исходящее влияние <input bind:value={outgoing} /></label>
        <button onclick={() => apply(() => { const a = Number(incoming), b = Number(outgoing); if (!incoming.trim() || !outgoing.trim() || !Number.isFinite(a) || !Number.isFinite(b)) throw new Error('Требуется конечное влияние'); return { kind: 'influence', time: exactTime(keyTime), incoming: a, outgoing: b } })}>Применить влияние</button>
      {:else}<p>Нет поддерживаемых параметров Filmcraft.</p>{/if}
    </fieldset>
    {#if selectedEffect && (maskIndex >= 0 || selectedEffect.maskable !== false)}
      <FilmcraftMaskMonitor {document} {sources} {token} {timeFrames} {timeTicks} createMask={maskIndex < 0} disabled={disabled || loading || busy || trackingBusy || report.locked} address={{ sequenceId, clipId, effectIndex: selectedEffect.index, effectKind: selectedEffect.kind, ...(maskIndex >= 0 ? { maskIndex } : {}), parameter: 'path' }} onbusy={value => monitorBusy = value} ondeselect={() => { if (open) { maskIndex = -1; parameterName = selectedEffect?.parameters[0]?.name ?? '' } }} onedit={(changed, created) => { if (open) { if (created !== undefined) { maskIndex = created; parameterName = 'path' } commitProject(changed) } }} />
    {/if}
    {#if selectedEffect && maskIndex >= 0}
      <FilmcraftMaskTrackingControls {document} {sources} {token} {timeFrames} {timeTicks} disabled={disabled || loading || busy || monitorBusy || report.locked} address={{ sequenceId, clipId, effectIndex: selectedEffect.index, effectKind: selectedEffect.kind, maskIndex, parameter: 'path' }} onbusy={value => trackingBusy = value} onedit={(changed, expected) => open ? commitProject(changed, expected) : false} />
    {/if}
  {/if}
</details>
