<script lang="ts">
  import { cancelJob, downloadFilmcraftVideo, exportNestedVideo, getJob, listNestedExports, nestedDurations, nestedExportCursorTimecode, retryFilmcraftVideoJob, setNestedExportPriority, moveNestedExport, type CaptionFileFormat, type NestedExportHistoryEntry, type NestedExportQueueEntry } from '$lib/api'
  import { nestedComposition, nestedHistory } from '$lib/state/nestedComposition.svelte'
  import { loadLibrary } from '$lib/state/store.svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { affectedTimelineIds } from '$lib/composition/timelineOrder'
  import CaptionFileExport from './CaptionFileExport.svelte'
  import type { CaptionTrack, CompositionCanvas } from '$lib/composition/types'
  import type { Job } from '$lib/types'
  let { token, disabled = false }: { token: string; disabled?: boolean } = $props()
  let withAudio = $state(false)
  let audioOnly = $state(false)
  let audioChannels = $state<1 | 2>(2)
  let audioCodec = $state<'wav' | 'aac' | 'opus'>('wav')
  let videoAudioCodec = $state<typeof audioCodec>('wav')
  let audioPcmDepth = $state<'float' | 's16' | 's24'>('float')
  let videoPcmDepth = $state<typeof audioPcmDepth>('float')
  const selectedPcmDepth = $derived(audioOnly ? audioPcmDepth : videoPcmDepth)
  const selectedAudioCodec = $derived(audioOnly ? audioCodec : videoAudioCodec)
  let bitrateKbps = $state(192)
  let audioSampleRate = $state(48000)
  const sampleRates = [32000, 44100, 48000, 88200, 96000, 192000]
  let videoAudioChannels = $state<1 | 2>(2)
  let videoAudioSampleRate = $state(48000)
  const selectedChannels = $derived(audioOnly ? audioChannels : videoAudioChannels)
  const selectedSampleRate = $derived(audioOnly ? audioSampleRate : videoAudioSampleRate)
  function selectAudioCodec(codec: typeof audioCodec) {
    if (audioOnly) {
      audioCodec = codec
      if (codec === 'opus' || (codec === 'aac' && audioSampleRate > 96000)) audioSampleRate = 48000
    } else {
      videoAudioCodec = codec
      if (codec === 'opus' || (codec === 'aac' && videoAudioSampleRate > 96000)) videoAudioSampleRate = 48000
    }
  }
  const maxAudioBitrate = $derived(selectedAudioCodec === 'aac'
    ? Math.min(320, Math.floor(selectedSampleRate * selectedChannels * 6 / 1000))
    : selectedChannels === 1 && selectedAudioCodec === 'opus' ? 256 : 320)
  const validAudioSettings = $derived((!audioOnly && !withAudio) || selectedAudioCodec === 'wav' ||
    (Number.isInteger(bitrateKbps) && bitrateKbps >= 32 && bitrateKbps <= maxAudioBitrate))
  let downloadName = $state('')
  let burnCaptions = $state(true)
  let startUs = $state(0)
  let endUs = $state(0)
  let duration = $state(0)
  let durationBusy = $state(false)
  let durationError = $state('')
  let busy = $state(false)
  let submitting = $state(false)
  let queuedJobId = $state('')
  let queueSubmissionError = $state('')
  const captionsByJob = new SvelteMap<string, { owner: string; snapshot: SubmittedCaptions }>()
  let monitoring = $state(false)
  let cancelling = $state(false)
  let retrying = $state(false)
  let error = $state('')
  let job = $state<Job | null>(null)
  let jobId = $state('')
  type SubmittedCaptions = {
    track: CaptionTrack
    canvas: Pick<CompositionCanvas, 'fps' | 'fpsRate'>
    range: { startUs: number; endUs: number }
    exactRange?: { startTick: string; endTick: string }
  }
  let submittedCaptions = $state.raw<SubmittedCaptions>()
  let downloading = $state(false)
  const lifecycle = new AbortController()
  let observation = new AbortController()
  let timer: ReturnType<typeof setTimeout> | undefined
  const compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  const validRange = $derived(Number.isSafeInteger(startUs) && Number.isSafeInteger(endUs) && startUs >= 0 && startUs < endUs && endUs <= duration)
  let useTimecode = $state(false)
  let timecodeStart = $state('00:00:00:00')
  let timecodeEnd = $state('')
  let timecodeBusy = $state(false)
  let timecodeError = $state('')
  let timecodeConversion: AbortController | undefined
  const timecodePattern = /^\d{2}:\d{2}:\d{2}[:;]\d{2,3}$/
  const canSubmitRange = $derived(useTimecode
    ? duration > 0 && timecodePattern.test(timecodeStart.trim()) && timecodePattern.test(timecodeEnd.trim())
    : validRange)
  const rangeLocked = $derived(submitting || retrying || durationBusy || timecodeBusy || disabled)
  const cursorUs = $derived(Math.min(duration, Math.max(0, nestedComposition.timeUs)))
  const captionContent = $derived(nestedComposition.document?.compositions[compositionId]?.content)
  const timecodeRate = $derived(captionContent?.canvas
    ? captionContent.canvas.fpsRate ?? { numerator: captionContent.canvas.fpsMilli, denominator: 1000 }
    : undefined)
  const captionTracks = $derived(captionContent?.captionTracks ?? [])
  let captionTrackId = $state('')
  const captionTrack = $derived(captionTracks.find(track => track.id === captionTrackId) ?? captionTracks[0])
  const sidecarFormats = ['srt', 'vtt', 'scc', 'mcc', 'stl', 'ttml', 'dfxp'] as const
  let sidecarFormat = $state<CaptionFileFormat | ''>('')
  let sidecarDropFrame = $state(true)
  let history = $state<NestedExportHistoryEntry[]>([])
  let queue = $state<NestedExportQueueEntry[]>()
  const queuePositions = $derived(new Map(queue?.map((entry, index) => [entry.jobId, index])))
  let historyOpen = $state(false)
  let historyRevision = 0
  let historyBusy = $state(false)
  let historyLoaded = $state(false)
  let nextHistoryCursor = $state<string | null>(null)
  let historyError = $state('')
  let queueBusy = $state(false)
  const statusLabels = {
    pending: 'В очереди', running: 'В работе', done: 'Готово', error: 'Ошибка', cancelled: 'Отменено', interrupted: 'Прервано',
  } satisfies Record<Job['status'], string>
  let reopenId = $state('')
  const validReopenId = $derived(/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(reopenId.trim()))
  const storedSidecar = $derived.by(() => {
    const result = job?.status === 'done' ? job.result : null
    if (!result || typeof result !== 'object' || !('captionSidecar' in result)) return
    const sidecar = result.captionSidecar
    if (!sidecar || typeof sidecar !== 'object' || !('extension' in sidecar) || !('bytesBase64' in sidecar)) return
    if (!sidecarFormats.some(extension => extension === sidecar.extension) || typeof sidecar.bytesBase64 !== 'string') return
    return sidecar as { extension: CaptionFileFormat; bytesBase64: string }
  })
  const hasCaptions = $derived(Object.values(nestedComposition.document?.compositions ?? {}).some(({ content }) =>
    content.captionTracks?.some(track => !track.hidden && track.clips.length) ||
    content.tracks.some(track => track.kind === 'text' && !track.hidden && track.clips.length &&
      content.metadata?.captionTextTracks?.includes(track.id))))
  const progress = $derived(Math.min(100, Math.max(0, job?.progress ?? 0)))
  const filename = $derived(job?.status === 'done' && job.result && typeof job.result === 'object' && 'filename' in job.result && typeof job.result.filename === 'string' ? job.result.filename : '')
  const outputFormat = $derived.by(() => {
    const result = job?.status === 'done' ? job.result : null
    const container = result && typeof result === 'object' && 'container' in result ? result.container : null
    return container === 'wav' || container === 'm4a' || container === 'opus' ? container : 'mkv'
  })
  const customFilename = $derived(job?.status === 'done' && job.result && typeof job.result === 'object'
    && 'downloadName' in job.result && typeof job.result.downloadName === 'string' ? job.result.downloadName : '')
  const canRetry = $derived(job && (job.status === 'error' || job.status === 'interrupted') &&
    ['timeout', 'tool_unavailable', 'process_exit', 'storage', 'interrupted'].includes(job.errorKind ?? ''))
  const outputId = $derived(job?.status === 'done' && job.result && typeof job.result === 'object' &&
    'id' in job.result && typeof job.result.id === 'string' ? job.result.id : '')
  $effect(() => {
    const project = nestedComposition.document
    const id = compositionId
    const controller = new AbortController()
    duration = 0; startUs = 0; endUs = 0; durationError = ''
    useTimecode = false; timecodeStart = '00:00:00:00'; timecodeEnd = ''
    timecodeConversion?.abort(); timecodeBusy = false; timecodeError = ''
    if (!project || !id) return
    durationBusy = true
    void nestedDurations(project, controller.signal).then((durations) => {
      if (controller.signal.aborted) return
      const value = durations[id]
      if (!Number.isSafeInteger(value) || value === undefined || value < 0) throw new Error('Некорректная длительность композиции')
      duration = value; endUs = value
    }).catch((failure) => {
      if (!controller.signal.aborted) durationError = failure instanceof Error ? failure.message : String(failure)
    }).finally(() => { if (!controller.signal.aborted) durationBusy = false })
    return () => controller.abort()
  })
  $effect(() => () => { lifecycle.abort(); observation.abort(); timecodeConversion?.abort(); clearTimeout(timer) })
  $effect(() => {
    if (!historyOpen) return
    const owner = token
    const watcher = new AbortController()
    let historyTimer: ReturnType<typeof setTimeout>
    const schedule = () => {
      if (!watcher.signal.aborted) historyTimer = setTimeout(() => void poll(), 2000)
    }
    async function poll() {
      try {
        if (historyBusy || queueBusy) return
        const revision = historyRevision
        const active = history.filter(entry => entry.status === 'pending' || entry.status === 'running')
        if (!active.length) return
        const [results, snapshot] = await Promise.all([
          Promise.allSettled(active.map(entry => getJob(entry.jobId, watcher.signal, owner))),
          queue ? listNestedExports(owner, watcher.signal).catch(() => undefined) : undefined,
        ])
        if (watcher.signal.aborted || token !== owner || revision !== historyRevision) return
        if (snapshot?.queue) queue = snapshot.queue
        const byId = new Map(results.flatMap(result => result.status === 'fulfilled'
          ? [[result.value.id, result.value.status] as const] : []))
        history = history.map(entry => {
          const status = byId.get(entry.jobId)
          return status && (entry.status === 'pending' || entry.status === 'running') ? { ...entry, status } : entry
        })
      } finally {
        schedule()
      }
    }
    historyTimer = setTimeout(() => {
      if (!watcher.signal.aborted) void refreshHistory().finally(schedule)
    }, 0)
    return () => {
      clearTimeout(historyTimer)
      watcher.abort()
    }
  })
  async function observe(): Promise<void> {
    if (!jobId || lifecycle.signal.aborted) return
    const { signal } = observation
    monitoring = true; error = ''
    try {
      const current = await getJob(jobId, signal, token)
      if (signal.aborted) return
      job = current
      if (current.status === 'done' || current.status === 'cancelled' || current.status === 'error' || current.status === 'interrupted') {
        busy = false; monitoring = false; cancelling = false
        if (current.status === 'error') error = current.error || 'Экспорт завершился с ошибкой'
        if (current.status === 'interrupted') error = 'Задача прервана сервером'
        if (current.status === 'done') void loadLibrary()
        return
      }
      timer = setTimeout(() => void observe(), 1000)
    } catch (failure) {
      if (signal.aborted) return
      monitoring = false
      error = failure instanceof Error ? failure.message : String(failure)
    }
  }
  async function setTimecodeFromCursor(boundary: 'start' | 'end') {
    const rate = timecodeRate
    if (rangeLocked || !rate || !Number.isSafeInteger(cursorUs)) return
    const request = new AbortController()
    timecodeConversion = request
    timecodeBusy = true
    timecodeError = ''
    const dropFrame = timecodeStart.includes(';') || timecodeEnd.includes(';') || captionContent?.metadata?.dropFrame === true
    try {
      const { timecode } = await nestedExportCursorTimecode(cursorUs, rate, dropFrame, request.signal)
      if (request.signal.aborted) return
      if (!timecodePattern.test(timecode)) throw new Error('Сервер вернул некорректный таймкод')
      if (boundary === 'start') timecodeStart = timecode
      else timecodeEnd = timecode
    } catch (failure) {
      if (!request.signal.aborted) timecodeError = failure instanceof Error ? failure.message : String(failure)
    } finally {
      if (timecodeConversion === request) timecodeBusy = false
    }
  }

  async function start(append = false): Promise<void> {
    const owner = token
    const project = nestedComposition.document
    if (!project || rangeLocked || !canSubmitRange || !validAudioSettings || nestedHistory.busy) return
    if (append ? !busy || !jobId : busy) return
    submitting = true
    queuedJobId = ''; queueSubmissionError = ''
    if (!append) {
      closeObservation()
      busy = true
    }
    try {
      const captionSnapshot: SubmittedCaptions | undefined = captionTrack && captionContent ? {
        track: JSON.parse(JSON.stringify(captionTrack)) as CaptionTrack,
        canvas: {
          fps: captionContent.canvas.fpsMilli / 1000,
          fpsRate: { ...(captionContent.canvas.fpsRate ?? { numerator: captionContent.canvas.fpsMilli, denominator: 1000 }) },
        },
        range: { startUs, endUs },
      } : undefined
      const hasCutout=Object.values(project.compositions).some(node=>node.content.tracks.some(track=>track.kind==='video'&&track.clips.some(clip=>clip.enabled!==false&&clip.concatCutout)))
      const savedProject=hasCutout&&nestedComposition.projectId&&nestedComposition.revision!==null?{projectId:nestedComposition.projectId,expectedRevision:nestedComposition.revision}:undefined
      const renderedOutputs = { ...nestedComposition.renderedOutputs }
      const document = audioOnly || burnCaptions || !hasCaptions ? project : { ...project, compositions: Object.fromEntries(
        Object.entries(project.compositions).map(([id, node]) => {
          const { content } = node
          const roles = new SvelteSet(content.metadata?.captionTextTracks)
          const native = content.captionTracks?.some(track => !track.hidden && track.clips.length)
          const text = content.tracks.some(track => track.kind === 'text' && !track.hidden && track.clips.length && roles.has(track.id))
          if (!native && !text) return [id, node]
          for (const affected of affectedTimelineIds(project, id)) delete renderedOutputs[affected]
          return [id, { ...node, content: {
            ...content,
            ...(native ? { captionTracks: content.captionTracks?.map(track => ({ ...track, hidden: true })) } : {}),
            ...(text ? { tracks: content.tracks.map(track => track.kind === 'text' && roles.has(track.id)
              ? { ...track, hidden: true } : track) } : {}),
          } }]
        }),
      ) }
      const args = [document, compositionId, startUs, endUs, renderedOutputs, owner, lifecycle.signal, audioOnly ? 'audio_only' : withAudio ? 'video_audio' : 'video_only'] as const
      const sidecar = sidecarFormat && captionTrack
        ? { trackId: captionTrack.id, format: sidecarFormat, dropFrame: sidecarDropFrame }
        : undefined
      const timecode = useTimecode ? { start: timecodeStart.trim(), end: timecodeEnd.trim() } : undefined
      const name = downloadName.trim()
      const compression = (audioOnly || withAudio) && selectedAudioCodec !== 'wav' ? { codec: selectedAudioCodec, bitrateKbps } : undefined
      const channels = (audioOnly || withAudio) && selectedChannels === 1 ? selectedChannels : undefined
      const sampleRate = (audioOnly || withAudio) && selectedSampleRate !== 48000 ? selectedSampleRate : undefined
      const pcmDepth = (audioOnly || withAudio) && selectedAudioCodec === 'wav' && selectedPcmDepth !== 'float' ? selectedPcmDepth : undefined
      const accepted = pcmDepth ? await exportNestedVideo(...args, savedProject, sidecar, timecode, name || undefined, compression, channels, sampleRate, pcmDepth)
        : sampleRate ? await exportNestedVideo(...args, savedProject, sidecar, timecode, name || undefined, compression, channels, sampleRate)
        : channels ? await exportNestedVideo(...args, savedProject, sidecar, timecode, name || undefined, compression, channels)
        : compression ? await exportNestedVideo(...args, savedProject, sidecar, timecode, name || undefined, compression)
        : name ? await exportNestedVideo(...args, savedProject, sidecar, timecode, name)
        : timecode ? await exportNestedVideo(...args, savedProject, sidecar, timecode)
        : sidecar ? await exportNestedVideo(...args, savedProject, sidecar)
        : savedProject ? await exportNestedVideo(...args, savedProject) : await exportNestedVideo(...args)
      if (captionSnapshot && timecode) {
        captionSnapshot.exactRange = accepted.captionRange
        if (accepted.startUs !== undefined && accepted.endUs !== undefined) {
          captionSnapshot.range = { startUs: accepted.startUs, endUs: accepted.endUs }
        }
      }
      if (lifecycle.signal.aborted || token !== owner) return
      if (captionSnapshot) captionsByJob.set(accepted.jobId, { owner, snapshot: captionSnapshot })
      if (append) {
        queuedJobId = accepted.jobId
      } else {
        submittedCaptions = captionSnapshot
        jobId = accepted.jobId
        void observe()
      }
      if (historyOpen) void refreshHistory()
    } catch (failure) {
      if (lifecycle.signal.aborted || token !== owner) return
      const message = failure instanceof Error ? failure.message : String(failure)
      if (append) queueSubmissionError = message
      else { busy = false; error = message }
    } finally {
      submitting = false
    }
  }
  async function refreshHistory(older = false) {
    if (historyBusy || queueBusy || (older && !nextHistoryCursor)) return
    const owner = token, cursor = older ? nextHistoryCursor : null
    historyRevision++
    historyBusy = true; historyError = ''
    try {
      const result = cursor ? await listNestedExports(owner, lifecycle.signal, cursor)
        : await listNestedExports(owner, lifecycle.signal)
      if (lifecycle.signal.aborted || token !== owner) return
      const loaded = new SvelteSet(older ? history.map(entry => entry.jobId) : [])
      history = older ? [...history, ...result.jobs.filter(entry => !loaded.has(entry.jobId))] : result.jobs
      queue = result.queue
      nextHistoryCursor = result.nextCursor
      historyLoaded = true
    } catch (failure) {
      if (!lifecycle.signal.aborted && token === owner) historyError = failure instanceof Error ? failure.message : String(failure)
    } finally { historyBusy = false }
  }
  async function changePriority(entry: NestedExportHistoryEntry) {
    if (queueBusy || historyBusy || disabled || entry.status !== 'pending') return
    const owner = token
    historyRevision++
    queueBusy = true
    historyError = ''
    try {
      const result = await setNestedExportPriority(entry.jobId, !(queue?.find(row => row.jobId === entry.jobId)?.raised ?? entry.raised), owner, lifecycle.signal)
      if (lifecycle.signal.aborted || token !== owner) return
      if (result.jobId !== entry.jobId || typeof result.raised !== 'boolean') {
        throw new Error('Некорректный ответ об изменении приоритета')
      }
      if (result.queue) queue = result.queue
      history = history.map(row => row.jobId === result.jobId ? { ...row, raised: result.raised } : row)
    } catch (failure) {
      if (!lifecycle.signal.aborted && token === owner) {
        historyError = failure instanceof Error ? failure.message : String(failure)
      }
    } finally {
      queueBusy = false
    }
  }
  function canMove(index: number, direction: 'earlier' | 'later') {
    const current = queue?.[index]
    const next = queue?.[direction === 'earlier' ? index - 1 : index + 1]
    return !!current && !!next && current.raised === next.raised
  }

  async function moveExport(entry: NestedExportHistoryEntry, direction: 'earlier' | 'later') {
    const position = queuePositions.get(entry.jobId)
    if (queueBusy || historyBusy || disabled || entry.status !== 'pending' ||
        position === undefined || !canMove(position, direction)) return
    const owner = token
    historyRevision++
    queueBusy = true
    historyError = ''
    try {
      const result = await moveNestedExport(entry.jobId, direction, owner, lifecycle.signal)
      if (lifecycle.signal.aborted || token !== owner) return
      if (result.jobId !== entry.jobId || result.applied !== true || !Array.isArray(result.queue)) {
        throw new Error('Некорректный ответ о перестановке экспорта')
      }
      queue = result.queue
    } catch (failure) {
      if (!lifecycle.signal.aborted && token === owner) {
        historyError = failure instanceof Error ? failure.message : String(failure)
      }
    } finally {
      queueBusy = false
    }
  }
  async function cancelQueuedExport(entry: NestedExportHistoryEntry) {
    if (queueBusy || historyBusy || disabled || entry.status !== 'pending') return
    const owner = token
    historyRevision++
    queueBusy = true
    historyError = ''
    try {
      await cancelJob(entry.jobId, owner)
      if (lifecycle.signal.aborted || token !== owner) return
      const current = await getJob(entry.jobId, lifecycle.signal, owner)
      if (lifecycle.signal.aborted || token !== owner) return
      history = history.map(row => row.jobId === entry.jobId ? { ...row, status: current.status } : row)
      if (current.status !== 'pending') queue = queue?.filter(row => row.jobId !== entry.jobId)
      if (current.status !== 'cancelled') historyError = 'Отмена экспорта пока не подтверждена'
    } catch (failure) {
      if (!lifecycle.signal.aborted && token === owner) {
        historyError = failure instanceof Error ? failure.message : String(failure)
      }
    } finally {
      queueBusy = false
    }
  }
  function reopen(id: string) {
    if (submitting || retrying || id === jobId) return
    closeObservation()
    const captions = captionsByJob.get(id)
    job = null; submittedCaptions = captions?.owner === token ? captions.snapshot : undefined; busy = true
    jobId = id
    void observe()
  }
  async function retry() {
    if (!jobId || disabled || busy || submitting || !canRetry) return
    const id = jobId
    busy = true; retrying = true; error = ''
    try {
      const accepted = await retryFilmcraftVideoJob(token, id, lifecycle.signal)
      if (lifecycle.signal.aborted) return
      job = null; jobId = accepted.jobId
      retrying = false
      if (historyLoaded) void refreshHistory()
      await observe()
    } catch (failure) {
      if (lifecycle.signal.aborted) return
      busy = false
      error = failure instanceof Error ? failure.message : String(failure)
    } finally { retrying = false }
  }
  function closeObservation() {
    observation.abort()
    clearTimeout(timer)
    observation = new AbortController()
    job = null; jobId = ''; busy = false; monitoring = false; cancelling = false
    submittedCaptions = undefined; error = ''
  }
  function saveFile(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url; link.download = filename
    document.body.append(link); link.click(); link.remove()
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }
  function downloadStoredCaptions() {
    if (!outputId || !storedSidecar) return
    error = ''
    try {
      const bytes = Uint8Array.from(atob(storedSidecar.bytesBase64), char => char.charCodeAt(0))
      saveFile(new Blob([bytes], { type: 'application/octet-stream' }), `${customFilename.replace(/\.(?:mkv|wav|m4a|opus)$/i, '') || outputId}.${storedSidecar.extension}`)
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure)
    }
  }
  async function downloadVideo() {
    if (!outputId || downloading) return
    const id = outputId
    const format = outputFormat
    const name = customFilename || `${id}.${format}`
    downloading = true; error = ''
    try {
      const blob = format === 'mkv' ? await downloadFilmcraftVideo(token, id, lifecycle.signal)
        : await downloadFilmcraftVideo(token, id, lifecycle.signal, format)
      if (lifecycle.signal.aborted || outputId !== id) return
      saveFile(blob, name)
    } catch (failure) {
      if (!lifecycle.signal.aborted && outputId === id) error = failure instanceof Error ? failure.message : String(failure)
    } finally { downloading = false }
  }
  async function cancel(): Promise<void> {
    if (!jobId || cancelling) return
    const { signal } = observation
    cancelling = true
    await cancelJob(jobId, token)
    if (!signal.aborted && !monitoring) void observe()
  }
</script>
<section aria-label="Экспорт вложенной композиции" class="nested-export">
  <span>{audioOnly ? (audioCodec === 'wav' ? 'WAV / PCM' : audioCodec === 'aac' ? 'M4A / AAC' : 'Ogg / Opus') : `FFV1 / MKV · ${withAudio ? (videoAudioCodec === 'wav' ? 'звук PCM' : videoAudioCodec === 'aac' ? 'звук AAC' : 'звук Opus') : 'без звука'}`}</span>
  <label>Имя файла <input type="text" bind:value={downloadName} placeholder="Автоматически" disabled={rangeLocked} /></label>
  <label><input aria-label="Добавить звук в нативный экспорт" type="checkbox" bind:checked={withAudio} disabled={audioOnly || submitting || retrying || disabled} /> Добавить звук</label>
  <label><input type="checkbox" bind:checked={audioOnly} disabled={submitting || retrying || disabled} /> Только звук</label>
  {#if audioOnly}
    <label>Формат звука
      <select aria-label="Формат звука" value={audioCodec} disabled={rangeLocked}
        onchange={event => selectAudioCodec(event.currentTarget.value as typeof audioCodec)}>
        <option value="wav">WAV (PCM)</option>
        <option value="aac">M4A (AAC)</option>
        <option value="opus">Ogg (Opus)</option>
      </select>
    </label>
  {:else if withAudio}
    <label>Кодек звука
      <select aria-label="Кодек звука" value={videoAudioCodec} disabled={rangeLocked}
        onchange={event => selectAudioCodec(event.currentTarget.value as typeof audioCodec)}>
        <option value="wav">PCM</option>
        <option value="aac">AAC</option>
        <option value="opus">Opus</option>
      </select>
    </label>
  {/if}
  {#if audioOnly || withAudio}
    <label>Каналы
      <select aria-label="Каналы звука" value={selectedChannels} disabled={rangeLocked}
        onchange={event => {
          const channels = event.currentTarget.value === '1' ? 1 : 2
          if (audioOnly) audioChannels = channels
          else videoAudioChannels = channels
        }}>
        <option value={2}>Стерео</option>
        <option value={1}>Моно</option>
      </select>
    </label>
    {#if selectedAudioCodec === 'wav'}
      <label>Разрядность PCM
        <select aria-label="Разрядность PCM" value={selectedPcmDepth} disabled={rangeLocked}
          onchange={event => {
            const depth = event.currentTarget.value as typeof audioPcmDepth
            if (audioOnly) audioPcmDepth = depth
            else videoPcmDepth = depth
          }}>
          <option value="float">32 бит, с плавающей точкой</option>
          <option value="s16">16 бит</option>
          <option value="s24">24 бита</option>
        </select>
      </label>
    {/if}
    {#if selectedAudioCodec !== 'opus'}
      <label>Частота дискретизации
        <select aria-label="Частота дискретизации" value={selectedSampleRate} disabled={rangeLocked}
          onchange={event => {
            const rate = Number(event.currentTarget.value)
            if (audioOnly) audioSampleRate = rate
            else videoAudioSampleRate = rate
          }}>
          {#each sampleRates.filter(rate => selectedAudioCodec === 'wav' || rate <= 96000) as rate (rate)}
            <option value={rate}>{rate / 1000} кГц</option>
          {/each}
        </select>
      </label>
    {/if}
  {/if}
  {#if (audioOnly || withAudio) && selectedAudioCodec !== 'wav'}
    <label>Битрейт, кбит/с
      <input type="number" min="32" max={maxAudioBitrate} step="1" bind:value={bitrateKbps}
        disabled={rangeLocked} aria-invalid={!validAudioSettings} />
    </label>
    {#if !validAudioSettings}<p role="alert">Битрейт должен быть от 32 до {maxAudioBitrate} кбит/с</p>{/if}
  {/if}
  {#if hasCaptions && !audioOnly}
    <label><input type="checkbox" checked={burnCaptions} onchange={event => burnCaptions = event.currentTarget.checked}
      disabled={submitting || retrying || disabled} />Вшить субтитры в видео</label>
  {/if}
  <label><input type="checkbox" bind:checked={useTimecode} disabled={rangeLocked} /> Диапазон по таймкоду</label>
  {#if useTimecode}
    <label>Начальный таймкод <input type="text" bind:value={timecodeStart} placeholder="00:00:00:00" disabled={rangeLocked} /></label>
    <button class="btn ghost sm" disabled={rangeLocked || !timecodeRate || duration === 0}
      onclick={() => void setTimecodeFromCursor('start')}>Начало по курсору</button>
    <label>Конечный таймкод <input type="text" bind:value={timecodeEnd} placeholder="00:00:00:00" disabled={rangeLocked} /></label>
    <button class="btn ghost sm" disabled={rangeLocked || !timecodeRate || duration === 0}
      onclick={() => void setTimecodeFromCursor('end')}>Конец по курсору</button>
    {#if timecodeError}<p role="alert">{timecodeError}</p>{/if}
  {:else}
    <label>От, мкс <input aria-label="Начало нативного экспорта" type="number" min="0" max={duration} step="1" bind:value={startUs} disabled={rangeLocked} /></label>
    <button class="btn ghost sm" disabled={rangeLocked || !Number.isSafeInteger(cursorUs) || !(cursorUs < endUs)}
      onclick={() => startUs = cursorUs}>Начало по курсору</button>
    <label>До, мкс <input aria-label="Конец нативного экспорта" type="number" min="0" max={duration} step="1" bind:value={endUs} disabled={rangeLocked} /></label>
    <button class="btn ghost sm" disabled={rangeLocked || !Number.isSafeInteger(cursorUs) || !(startUs < cursorUs)}
      onclick={() => endUs = cursorUs}>Конец по курсору</button>
    <button class="btn ghost sm" disabled={rangeLocked || duration === 0 || (startUs === 0 && endUs === duration)}
      onclick={() => { startUs = 0; endUs = duration }}>Весь таймлайн</button>
  {/if}
  <button class="btn primary sm" disabled={busy || rangeLocked || nestedHistory.busy || !canSubmitRange || !validAudioSettings} onclick={() => void start()}>{audioOnly ? 'Экспортировать звук' : 'Экспортировать видео'}</button>
  {#if busy && jobId && !retrying}
    <button class="btn sm" disabled={rangeLocked || nestedHistory.busy || !canSubmitRange || !validAudioSettings}
      onclick={() => void start(true)}>Добавить экспорт в очередь</button>
  {/if}
  {#if queuedJobId}<span role="status">Добавлена задача экспорта: {queuedJobId}</span>{/if}
  {#if queueSubmissionError}<p role="alert">{queueSubmissionError}</p>{/if}
  {#if retrying}
    <span role="status">Повторный запуск экспорта…</span>
  {:else if busy && jobId}
    <progress aria-label="Прогресс нативного экспорта" max="100" value={progress}></progress>
    <span>{Math.round(progress)}% {cancelling ? '· отмена запрошена' : ''}</span>
    <button class="btn ghost sm" disabled={cancelling} onclick={() => void cancel()}>Отменить экспорт</button>
    {#if !monitoring}
      <button class="btn ghost sm" onclick={() => void observe()}>Проверить задачу</button>
      <button class="btn ghost sm" onclick={closeObservation}>Закрыть задачу</button>
    {/if}
  {/if}
  {#if jobId}<span class="job-id">Задача: {jobId}</span>{/if}
  {#if filename}<span role="status">Готово в медиатеке: {customFilename || filename}</span>{/if}
  {#if outputId}
    <button class="btn ghost sm" disabled={downloading} onclick={() => void downloadVideo()}>{outputFormat === 'mkv' ? 'Скачать готовое видео' : 'Скачать готовый звук'}</button>
  {/if}
  {#if outputId && storedSidecar}
    <button class="btn ghost sm" onclick={downloadStoredCaptions}>Скачать субтитры {storedSidecar.extension.toUpperCase()}</button>
  {/if}
  <details ontoggle={event => { historyOpen = event.currentTarget.open }}>
    <summary>Недавние задачи экспорта</summary>
    <button class="btn ghost sm" disabled={historyBusy || queueBusy} onclick={() => void refreshHistory()}>Обновить список задач</button>
    {#if historyError}<p role="alert">{historyError}</p>{/if}
    {#if historyLoaded && !history.length}<p>Задач экспорта пока нет</p>{/if}
    {#if history.length}
      <ul>
        {#each history as entry (entry.jobId)}
          {@const position = queuePositions.get(entry.jobId)}
          <li><button class="btn ghost sm" disabled={submitting || retrying || entry.jobId === jobId} aria-current={entry.jobId === jobId ? 'true' : undefined} onclick={() => reopen(entry.jobId)}>
            {#if entry.downloadName}{entry.downloadName} · {/if}{new Date(entry.createdAt * 1000).toLocaleString('ru-RU')} · {statusLabels[entry.status]}
          </button>
          {#if entry.status === 'pending'}
            <button class="btn ghost sm" disabled={historyBusy || queueBusy || disabled}
              onclick={() => void cancelQueuedExport(entry)}>Отменить задачу</button>
            <button class="btn ghost sm" disabled={historyBusy || queueBusy || disabled}
              onclick={() => void changePriority(entry)}>
              {(position === undefined ? entry.raised : queue?.[position]?.raised) ? 'Вернуть обычный приоритет' : 'Повысить приоритет'}
            </button>
            {#if position !== undefined}
              <span>Место в вашей очереди: {position + 1}</span>
              <button class="btn ghost sm" disabled={historyBusy || queueBusy || disabled || !canMove(position, 'earlier')}
                onclick={() => void moveExport(entry, 'earlier')}>Раньше</button>
              <button class="btn ghost sm" disabled={historyBusy || queueBusy || disabled || !canMove(position, 'later')}
                onclick={() => void moveExport(entry, 'later')}>Позже</button>
            {/if}
          {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if nextHistoryCursor}
      <button class="btn ghost sm" disabled={historyBusy || queueBusy} onclick={() => void refreshHistory(true)}>Показать более ранние задачи</button>
    {/if}
    <label>Идентификатор задачи<input type="text" bind:value={reopenId} disabled={submitting || retrying} /></label>
    <button class="btn ghost sm" disabled={submitting || retrying || !validReopenId || reopenId.trim() === jobId} onclick={() => reopen(reopenId.trim())}>Открыть задачу</button>
  </details>
  {#if filename && submittedCaptions && !storedSidecar}
    <details aria-label="Субтитры к готовому видео">
      <summary>Субтитры к готовому видео · {submittedCaptions.track.name}</summary>
      <CaptionFileExport composition={{ canvas: submittedCaptions.canvas, captionTracks: [submittedCaptions.track] }}
        range={submittedCaptions.range} exactRange={submittedCaptions.exactRange} basename={(customFilename || filename).replace(/\.(?:mkv|wav|m4a|opus)$/i, '')} {disabled} />
    </details>
  {/if}
  {#if canRetry}
    <button class="btn ghost sm" disabled={busy || submitting || disabled} onclick={() => void retry()}>Повторить экспорт</button>
  {/if}
  {#if job?.status === 'cancelled'}<span role="status">Экспорт отменён</span>{/if}
  {#if captionContent && captionTrack}
    <details>
      <summary>Субтитры для диапазона видео</summary>
      <label>Дорожка для диапазона видео
        <select aria-label="Дорожка для диапазона видео" value={captionTrack.id} disabled={disabled}
          onchange={event => captionTrackId = event.currentTarget.value}>
          {#each captionTracks as track (track.id)}<option value={track.id}>{track.name}</option>{/each}
        </select>
      </label>
      <label>Файл субтитров в задаче
        <select aria-label="Файл субтитров в задаче" value={sidecarFormat} disabled={disabled || submitting || retrying}
          onchange={event => sidecarFormat = event.currentTarget.value as CaptionFileFormat | ''}>
          <option value="">Без отдельного файла</option>
          {#each sidecarFormats as format (format)}<option value={format}>{format.toUpperCase()}</option>{/each}
        </select>
      </label>
      {#if sidecarFormat === 'scc' || sidecarFormat === 'mcc'}
        <label><input type="checkbox" checked={sidecarDropFrame} disabled={submitting || retrying || disabled}
          onchange={event => sidecarDropFrame = event.currentTarget.checked} />Drop-frame файла субтитров</label>
      {/if}
      {#if validRange && !useTimecode && !durationBusy}
        {#key `${compositionId}/${captionTrack.id}`}
          <CaptionFileExport composition={{ canvas: {
            fps: captionContent.canvas.fpsMilli / 1000,
            fpsRate: captionContent.canvas.fpsRate ?? { numerator: captionContent.canvas.fpsMilli, denominator: 1000 },
          }, captionTracks }} activeId={captionTrack.id} range={{startUs,endUs}} {disabled} />
        {/key}
      {/if}
    </details>
  {/if}
  {#if durationError || error}<p role="alert">{durationError || error}</p>{/if}
</section>
<style>
  .nested-export { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin-block: 0.5rem; font-size: 0.8rem; }
  label { display: flex; align-items: center; gap: 0.25rem; }
  input[type="number"] { width: 8rem; }
  progress { width: 6rem; }
  .job-id { overflow-wrap: anywhere; }
  p { flex-basis: 100%; margin: 0; }
</style>
