<script lang="ts">
  import FilmcraftExportQueue from './FilmcraftExportQueue.svelte'
  import FilmcraftExportPresetLibrary from './FilmcraftExportPresetLibrary.svelte'
  import { startFilmcraftQuickExportJob, startFilmcraftVideoJob, getFilmcraftVideoJob, cancelFilmcraftVideoJob, retryFilmcraftVideoJob, downloadFilmcraftVideo, listFilmcraftVideoJobs, type FilmcraftVideoRequest, type FilmcraftVideoHistoryEntry, type FilmcraftExportPreset, type FilmcraftScaling, type FilmcraftExportMetadata, type FilmcraftTextOverlay, type FilmcraftOverlayPlacement, type FilmcraftImageOverlay, type FilmcraftBitrateSettings } from '$lib/api'
  import type { Job, MediaEntry } from '$lib/types'
  let { document, sequenceId, sources, token = null, initialFrames = '24', disabled = false, imageSources = [] }: { document: string; sequenceId: string; sources: Record<string, string>; token?: string | null; initialFrames?: string; disabled?: boolean; imageSources?: MediaEntry[] } = $props()
  let preset = $state<FilmcraftExportPreset>('lossless')
  let mjpegQuality = $state('90')
  let libraryPreset = $state('')
  let quickPresetName = $state('')
  $effect(() => { void token; libraryPreset = ''; quickPresetName = '' })
  const audioOnly = $derived(['wav16', 'wav24', 'aiff16'].includes(preset))
  const imageSequence = $derived(preset.endsWith('_sequence'))
  const silentVisual = $derived(imageSequence || preset === 'gif')
  $effect(() => { if (silentVisual) withAudio = false; else if (audioOnly) withAudio = true })
  const fixedPreset = $derived(!['lossless', 'mjpeg', 'hevc', 'adaptive_high', 'adaptive_medium', 'adaptive_low'].includes(preset) && !preset.startsWith('prores_') && !preset.startsWith('dnxhr_') && !preset.startsWith('apv') && !preset.startsWith('mxf_') && !imageSequence && !audioOnly)
  let customSize = $state(false)
  let customFrameRate = $state(false)
  const h264 = $derived(['adaptive_high', 'adaptive_medium', 'adaptive_low', 'high_quality1080', 'high_quality2160', 'youtube1080', 'youtube2160', 'vimeo1080', 'vimeo2160', 'social_vertical', 'mxf_h264'].includes(preset))
  type H264Profile = 'baseline' | 'main' | 'high'
  const hevc = $derived(preset === 'hevc')
  const h26x = $derived(h264 || hevc)
  let h264Profile = $state<H264Profile>('high')
  const h264Movie = $derived(h26x && preset !== 'mxf_h264')
  const intraMovie = $derived(preset === 'mjpeg' || preset.startsWith('prores_') || preset.startsWith('dnxhr_') || preset.startsWith('apv'))
  const mxfAudio = $derived(preset.startsWith('mxf_'))
  const nativeAudio = $derived(intraMovie || mxfAudio)
  let multiplexer = $state<'mp4' | 'mov'>('mp4')
  let movieAudioCodec = $state<'auto' | 'aac' | 'pcm'>('auto')
  let pcmAudioBits = $state<16 | 24 | null>(null)
  const movieAudioBits = $derived(pcmAudioBits ?? (nativeAudio ? 24 : 16))
  let movieAudioKbps = $state('320')
  let movieAudioChannels = $state(2)
  const effectivePcmBits = $derived(pcmAudioBits ?? (preset === 'wav24' ? 24 : 16))
  const aacRates = [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000]
  let movieAudioRate = $state(48000)
  const sequenceAudioRate = $derived.by(() => {
    try {
      const rate = JSON.parse(document)?.project?.items?.[sequenceId]?.kind?.Sequence?.settings?.sample_rate
      return Number.isInteger(rate) && rate >= 8000 && rate <= 192000 ? rate as number : 48000
    } catch { return 48000 }
  })
  const effectiveMovieRate = $derived(movieAudioRate || sequenceAudioRate)
  const moviePcm = $derived(mxfAudio || (intraMovie ? movieAudioCodec !== 'aac' : multiplexer === 'mov' && movieAudioCodec === 'pcm'))
  let customBitrate = $state(false)
  let bitrateMode = $state<FilmcraftBitrateSettings['mode']>('vbr1Pass')
  let bitrateTarget = $state('10000')
  let customMaxBitrate = $state(false)
  let bitrateMax = $state('15000')
  let adaptiveBitrate = $state(false)
  let adaptiveBpp = $state<number | undefined>(0.2)
  let customKeyframeDistance = $state(false)
  let keyframeDistance = $state('48')
  let rateNumerator = $state('25')
  let rateDenominator = $state('1')
  let outputWidth = $state('1920')
  let outputHeight = $state('1080')
  let scaling = $state<FilmcraftScaling>('scaleToFit')
  let customPixelAspect = $state(false)
  let pixelNumerator = $state('1')
  let pixelDenominator = $state('1')
  let maxRenderQuality = $state(false)
  let metadata = $state<Required<FilmcraftExportMetadata>>({ title: '', creator: '', copyright: '', description: '', comment: '' })
  const metadataFields: Array<{ key: keyof FilmcraftExportMetadata; label: string; multiline?: boolean }> = [{ key: 'title', label: 'Название' }, { key: 'creator', label: 'Автор' }, { key: 'copyright', label: 'Авторские права' }, { key: 'description', label: 'Описание', multiline: true }, { key: 'comment', label: 'Комментарий', multiline: true }]
  const metadataSizes = $derived(Object.values(metadata).map(value => new TextEncoder().encode(value).length))
  const metadataValid = $derived(metadataSizes.every(size => size <= 16384) && metadataSizes.reduce((sum, size) => sum + size, 0) <= 65536 && Object.values(metadata).every(value => !value.includes('\0')))
  const hasMetadata = $derived(Object.values(metadata).some(value => value.trim().length > 0))
  let limiterEnabled = $state(false)
  let limiterMin = $state<number | undefined>(0)
  let limiterMax = $state<number | undefined>(100)
  const limiterValid = $derived(!limiterEnabled || (limiterMin !== undefined && limiterMax !== undefined && Number.isFinite(limiterMin) && Number.isFinite(limiterMax) && limiterMin >= 0 && limiterMin <= 100 && limiterMax >= 0 && limiterMax <= 100))
  let loudnessEnabled = $state(false)
  let loudnessTarget = $state<number | undefined>(-23)
  let loudnessPeak = $state<number | undefined>(-2)
  const loudnessValid = $derived(loudnessTarget !== undefined && loudnessPeak !== undefined && Number.isFinite(loudnessTarget) && Number.isFinite(loudnessPeak) && loudnessPeak >= -24 && loudnessPeak <= 0)
  const overlayKinds = [{ key: 'nameOverlay', label: 'Подпись' }, { key: 'timecodeOverlay', label: 'Таймкод' }] as const
  const placements: Array<[FilmcraftOverlayPlacement, string]> = [['topLeft', 'Сверху слева'], ['topCenter', 'Сверху по центру'], ['topRight', 'Сверху справа'], ['centerLeft', 'По центру слева'], ['center', 'По центру'], ['centerRight', 'По центру справа'], ['bottomLeft', 'Снизу слева'], ['bottomCenter', 'Снизу по центру'], ['bottomRight', 'Снизу справа']]
  const defaultOverlay = (): FilmcraftTextOverlay => ({ enabled: false, text: '', placement: 'bottomCenter', offset: [0, 0], sizePercent: 5, opacity: 100 })
  let overlays = $state({ nameOverlay: defaultOverlay(), timecodeOverlay: defaultOverlay() })
  const overlaysValid = $derived(Object.values(overlays).every(o => !o.enabled || (new TextEncoder().encode(o.text).length <= 16000 && !o.text.includes('\0') && [o.offset[0], o.offset[1], o.sizePercent, o.opacity].every(Number.isFinite) && o.offset.every(n => Math.abs(n) <= 1000000))))
  let imageOverlay = $state<FilmcraftImageOverlay>({ enabled: false, sourceId: '', placement: 'topRight', offset: [0, 0], sizePercent: 15, opacity: 100 })
  const imageOverlayValid = $derived(!imageOverlay.enabled || (imageSources.some(source => source.id === imageOverlay.sourceId) && [imageOverlay.offset[0], imageOverlay.offset[1], imageOverlay.sizePercent, imageOverlay.opacity].every(Number.isFinite) && imageOverlay.offset.every(n => Math.abs(n) <= 1000000)))
  let withAudio = $state(false)
  let startFrames = $state('0')
  let frames = $state('24')
  let busy = $state(false)
  let submitting = $state(false)
  let submitError = $state('')
  let monitoring = $state(false)
  let cancelling = $state(false)
  let retrying = $state(false)
  let downloading = $state(false)
  let jobId = $state('')
  let job = $state<Job | null>(null)
  let error = $state('')
  let history = $state<FilmcraftVideoHistoryEntry[]>([])
  let nextCursor = $state<string | null>(null)
  let historyBusy = $state(false)
  let historyError = $state('')
  type Session = { token: string | null; controller: AbortController; poll: number; history: number; submit: number; timer?: ReturnType<typeof setTimeout> }
  let active: Session | null = null
  let saved: Array<{ url: string; timer: ReturnType<typeof setTimeout> }> = []
  const integerWithin = (value: string, max: number) => /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) > 0 && Number(value) <= max
  const frameRateValid = $derived(!customFrameRate || (integerWithin(rateNumerator, 2147483647) && integerWithin(rateDenominator, 2147483647) && Number(rateNumerator) / Number(rateDenominator) >= 1 && Number(rateNumerator) / Number(rateDenominator) <= 60))
  const keyframeValid = $derived(!h26x || !customKeyframeDistance || integerWithin(keyframeDistance, 4294967295))
  const bitrateValid = $derived((!hevc || bitrateMode !== 'vbr2Pass') && (!h26x || !customBitrate || ((adaptiveBitrate ? adaptiveBpp !== undefined && Number.isFinite(adaptiveBpp) && adaptiveBpp > 0 : integerWithin(bitrateTarget, 1000000) && Number(bitrateTarget) >= 100) && (bitrateMode === 'cbr' || !customMaxBitrate || integerWithin(bitrateMax, 1000000)))))
  const movieAudioValid = $derived(!withAudio || (!h264Movie && !nativeAudio) || (moviePcm || (aacRates.includes(effectiveMovieRate) && integerWithin(movieAudioKbps, 512) && Number(movieAudioKbps) >= 32)))
  const manualValid = $derived((preset !== 'mjpeg' || integerWithin(mjpegQuality, 100)) && (!withAudio || silentVisual || !loudnessEnabled || loudnessValid) && (audioOnly || (metadataValid && limiterValid && overlaysValid && imageOverlayValid && frameRateValid && keyframeValid && bitrateValid && movieAudioValid)) && /^-?\d+$/.test(startFrames) && /^\d+$/.test(frames) && Number.isSafeInteger(Number(frames)) && Number(frames) > 0 && Number(frames) <= 360000 && (audioOnly || !customSize || (integerWithin(outputWidth, 3840) && integerWithin(outputHeight, 2160))) && (audioOnly || !customPixelAspect || (integerWithin(pixelNumerator, (preset.startsWith('dnxhr_') || preset.startsWith('prores_') || preset.startsWith('apv')) ? 4294967295 : 2147483647) && integerWithin(pixelDenominator, (preset.startsWith('dnxhr_') || preset.startsWith('prores_') || preset.startsWith('apv')) ? 4294967295 : 2147483647))))
  const valid = $derived(libraryPreset ? /^-?\d+$/.test(startFrames) && /^\d+$/.test(frames) && Number.isSafeInteger(Number(frames)) && Number(frames) > 0 && Number(frames) <= 360000 : manualValid)
  const progress = $derived(Number.isFinite(job?.progress) ? Math.max(0, Math.min(100, job?.progress ?? 0)) : 0)
  const outputId = $derived(job?.status === 'done' && job.result && typeof job.result === 'object' && 'id' in job.result && typeof job.result.id === 'string' ? job.result.id : '')
  const captionSidecar = $derived.by(() => {
    const result = job?.status === 'done' ? job.result : null
    if (!result || typeof result !== 'object' || !('captionSidecar' in result)) return null
    const sidecar = result.captionSidecar
    if (!sidecar || typeof sidecar !== 'object' || !('extension' in sidecar) || !('bytesBase64' in sidecar)) return null
    if (typeof sidecar.extension !== 'string' || !['srt', 'vtt', 'scc', 'mcc', 'stl', 'ttml', 'dfxp'].includes(sidecar.extension) || typeof sidecar.bytesBase64 !== 'string') return null
    return { extension: sidecar.extension, bytesBase64: sidecar.bytesBase64 }
  })
  const outputFormat = $derived.by(() => {
    const result = job?.result
    return result && typeof result === 'object' && 'container' in result && (result.container === 'mp4' || result.container === 'mov' || result.container === 'mxf' || result.container === 'zip' || result.container === 'gif' || result.container === 'wav' || result.container === 'aiff') ? result.container : 'mkv'
  })
  const canRetry = $derived(!!job && ['error', 'interrupted'].includes(job.status) && ['timeout', 'tool_unavailable', 'process_exit', 'storage', 'interrupted'].includes(job.errorKind ?? ''))
  $effect(() => { frames = initialFrames; startFrames = '0' })
  $effect(() => {
    const session: Session = { token, controller: new AbortController(), poll: 0, history: 0, submit: 0 }
    active = session; busy = false; submitting = false; submitError = ''; monitoring = false; cancelling = false; retrying = false; downloading = false; job = null; jobId = ''; error = ''
    history = []; nextCursor = null; historyBusy = false; historyError = ''; if (session.token) void loadHistory(session)
    return () => {
      session.controller.abort(); clearTimeout(session.timer)
      for (const item of saved) { clearTimeout(item.timer); URL.revokeObjectURL(item.url) }
      saved = []
      if (active === session) active = null
    }
  })
  function current(session: Session) { return active === session && !session.controller.signal.aborted }
  async function loadHistory(session: Session, append = false) {
    if (!current(session) || !session.token) return
    const cursor = append ? nextCursor ?? undefined : undefined
    const version = ++session.history
    historyBusy = true; historyError = ''
    try {
      const page = await listFilmcraftVideoJobs(session.token, cursor, session.controller.signal)
      if (!current(session) || session.history !== version) return
      history = append ? [...history, ...page.jobs.filter(row => !history.some(old => old.jobId === row.jobId))] : page.jobs
      nextCursor = page.nextCursor
    } catch (failure) { if (current(session) && session.history === version) historyError = failure instanceof Error ? failure.message : String(failure) }
    finally { if (current(session) && session.history === version) historyBusy = false }
  }
  async function openHistory(row: FilmcraftVideoHistoryEntry) {
    const session = active
    if (!session?.token || submitting || retrying || downloading) return
    clearTimeout(session.timer); ++session.poll
    jobId = row.jobId; job = null; busy = true; cancelling = false; error = ''
    await observe(session, row.jobId)
  }
  function statusText(status: Job['status']) {
    return { pending: 'В очереди', running: 'Выполняется', done: 'Готово', error: 'Ошибка', cancelled: 'Отменён', interrupted: 'Прерван' }[status]
  }
  async function observe(session: Session, id: string) {
    if (!current(session) || !session.token) return
    clearTimeout(session.timer)
    const version = ++session.poll
    monitoring = true; error = ''
    try {
      const value = await getFilmcraftVideoJob(session.token, id, session.controller.signal)
      if (!current(session) || version !== session.poll) return
      job = value
      history = history.map(row => row.jobId === id ? { ...row, job: value } : row)
      if (['done', 'error', 'cancelled', 'interrupted'].includes(value.status)) {
        busy = false; monitoring = false; cancelling = false
        void loadHistory(session)
        if (value.status === 'error' || value.status === 'interrupted') error = value.error || 'Экспорт завершился с ошибкой'
      } else session.timer = setTimeout(() => void observe(session, id), 1000)
    } catch (failure) {
      if (current(session) && version === session.poll) {
        monitoring = false; error = failure instanceof Error ? failure.message : String(failure)
        if (failure && typeof failure === 'object' && 'status' in failure && [401, 403, 404].includes(Number(failure.status))) { busy = false; job = null; void loadHistory(session) }
      }
    }
  }
  async function quickExport() {
    const session = active
    if (!session?.token || !current(session) || submitting || retrying || disabled) return
    const version = ++session.submit
    submitting = true; submitError = ''
    try {
      const accepted = await startFilmcraftQuickExportJob(session.token, { document, sequenceId, sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)), ...(libraryPreset ? { preset: libraryPreset } : {}) }, session.controller.signal)
      if (!current(session) || session.submit !== version) return
      quickPresetName = accepted.presetName
      clearTimeout(session.timer); ++session.poll
      jobId = accepted.jobId; job = null; busy = true; cancelling = false; error = ''; submitting = false
      void loadHistory(session)
      await observe(session, accepted.jobId)
    } catch (failure) { if (current(session) && session.submit === version) submitError = failure instanceof Error ? failure.message : String(failure) }
    finally { if (current(session) && session.submit === version) submitting = false }
  }
  function buildRequest(): FilmcraftVideoRequest {
    return libraryPreset ? { document, sequenceId, startFrames, frames: Number(frames), sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)), libraryPreset } : { document, sequenceId, startFrames, frames: Number(frames), sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)), ...(withAudio ? { withAudio: true } : {}), ...(h264Movie && multiplexer === 'mov' ? { multiplexer } : {}), ...(audioOnly && (movieAudioRate !== 48000 || movieAudioChannels !== 2 || pcmAudioBits !== null) ? { audio: { channels: movieAudioChannels, bits: effectivePcmBits, sampleRate: movieAudioRate } } : {}), ...(withAudio && h264Movie && (movieAudioCodec !== 'auto' || movieAudioBits !== 16 || movieAudioKbps !== '320' || movieAudioRate !== 48000 || movieAudioChannels !== 2) ? { h264Audio: { codec: movieAudioCodec, bits: movieAudioBits, bitrateKbps: moviePcm ? 320 : Number(movieAudioKbps), ...(movieAudioChannels !== 2 ? { channels: movieAudioChannels } : {}), sampleRate: movieAudioRate } } : {}), ...(withAudio && nativeAudio ? { audio: { codec: mxfAudio ? 'pcm' : movieAudioCodec, bits: movieAudioBits, channels: movieAudioChannels, sampleRate: movieAudioRate, bitrateKbps: moviePcm ? 320 : Number(movieAudioKbps) } } : {}), ...(h26x && customBitrate ? { bitrate: { mode: bitrateMode, targetKbps: adaptiveBitrate ? 10000 : Number(bitrateTarget), ...(adaptiveBitrate ? { adaptiveBpp: Number(adaptiveBpp) } : {}), ...(bitrateMode !== 'cbr' && customMaxBitrate ? { maxKbps: Number(bitrateMax) } : {}) } } : {}), ...(h264 && h264Profile !== 'high' ? { h264Profile } : {}), ...(h26x && customKeyframeDistance ? { keyframeDistance: Number(keyframeDistance) } : {}), ...(!audioOnly && customFrameRate ? { frameRate: { numerator: Number(rateNumerator), denominator: Number(rateDenominator) } } : {}), ...(!audioOnly && imageOverlay.enabled ? { imageOverlay: { ...imageOverlay, offset: [...imageOverlay.offset] as [number, number] } } : {}), ...(!audioOnly && overlays.nameOverlay.enabled ? { nameOverlay: { ...overlays.nameOverlay, offset: [...overlays.nameOverlay.offset] as [number, number] } } : {}), ...(!audioOnly && overlays.timecodeOverlay.enabled ? { timecodeOverlay: { ...overlays.timecodeOverlay, offset: [...overlays.timecodeOverlay.offset] as [number, number] } } : {}), ...(withAudio && !silentVisual && loudnessEnabled ? { loudness: { enabled: true, targetLufs: Number(loudnessTarget), truePeakDbtp: Number(loudnessPeak) } } : {}), ...(preset === 'mjpeg' ? { mjpegQuality: Number(mjpegQuality) } : {}), ...(preset !== 'lossless' ? { preset } : {}), ...(!audioOnly && customSize ? { frameSize: [Number(outputWidth), Number(outputHeight)] as [number, number] } : {}), ...(!audioOnly && (fixedPreset || scaling !== 'scaleToFit') ? { scaling } : {}), ...(!audioOnly && customPixelAspect ? { pixelAspect: [Number(pixelNumerator), Number(pixelDenominator)] as [number, number] } : {}), ...(!audioOnly && maxRenderQuality ? { maxRenderQuality: true } : {}), ...(!audioOnly && hasMetadata ? { metadata: { ...metadata } } : {}), ...(!audioOnly && limiterEnabled ? { videoLimiter: { enabled: true, minPercent: Number(limiterMin), maxPercent: Number(limiterMax) } } : {}) }
  }
  async function start() {
    const session = active
    if (!session?.token || !current(session) || submitting || retrying || disabled || !valid) return
    const version = ++session.submit
    submitting = true; submitError = ''
    try {
      const accepted = await startFilmcraftVideoJob(session.token, buildRequest(), session.controller.signal)
      if (!current(session) || session.submit !== version) return
      clearTimeout(session.timer); ++session.poll
      jobId = accepted.jobId; job = null; busy = true; cancelling = false; error = ''; submitting = false
      void loadHistory(session)
      await observe(session, accepted.jobId)
    } catch (failure) {
      if (current(session) && session.submit === version) submitError = failure instanceof Error ? failure.message : String(failure)
    } finally { if (current(session) && session.submit === version) submitting = false }
  }
  async function retry() {
    const session = active
    if (!session?.token || !jobId || submitting || busy || retrying || !canRetry) return
    const id = jobId; const previous = job
    retrying = true; busy = true; error = ''
    try {
      const accepted = await retryFilmcraftVideoJob(session.token, id, session.controller.signal)
      if (!current(session) || jobId !== id) return
      job = null; jobId = accepted.jobId
      void loadHistory(session)
      await observe(session, accepted.jobId)
    } catch (failure) {
      if (current(session) && jobId === id) { job = previous; busy = false; error = failure instanceof Error ? failure.message : String(failure) }
    } finally { if (current(session)) retrying = false }
  }
  async function cancel() {
    const session = active
    if (!session?.token || !jobId || cancelling) return
    const id = jobId
    cancelling = true; error = ''
    try { await cancelFilmcraftVideoJob(session.token, id, session.controller.signal); if (current(session) && jobId === id) await observe(session, id) }
    catch (failure) { if (current(session) && jobId === id) { cancelling = false; error = failure instanceof Error ? failure.message : String(failure) } }
  }
  async function download() {
    const session = active
    if (!session?.token || !outputId || downloading) return
    downloading = true; error = ''
    try {
      const id = outputId; const format = outputFormat
      const blob = format === 'mkv' ? await downloadFilmcraftVideo(session.token, id, session.controller.signal) : await downloadFilmcraftVideo(session.token, id, session.controller.signal, format)
      if (!current(session)) return
      const url = URL.createObjectURL(blob)
      const anchor = window.document.createElement('a'); anchor.href = url; anchor.download = `${id}.${format}`
      window.document.body.append(anchor); anchor.click(); anchor.remove()
      saved.push({ url, timer: setTimeout(() => { URL.revokeObjectURL(url); saved = saved.filter(item => item.url !== url) }, 1000) })
    } catch (failure) { if (current(session)) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (current(session)) downloading = false }
  }
  function downloadCaptions() {
    if (!outputId || !captionSidecar) return
    try {
      const binary = atob(captionSidecar.bytesBase64)
      const bytes = Uint8Array.from(binary, character => character.charCodeAt(0))
      const type = captionSidecar.extension === 'stl' ? 'application/octet-stream' : ['ttml', 'dfxp'].includes(captionSidecar.extension) ? 'application/ttml+xml' : captionSidecar.extension === 'vtt' ? 'text/vtt' : 'text/plain'
      const url = URL.createObjectURL(new Blob([bytes], { type }))
      const anchor = window.document.createElement('a'); anchor.href = url; anchor.download = `${outputId}.${captionSidecar.extension}`
      window.document.body.append(anchor); anchor.click(); anchor.remove()
      saved.push({ url, timer: setTimeout(() => { URL.revokeObjectURL(url); saved = saved.filter(item => item.url !== url) }, 1000) })
    } catch (failure) { error = failure instanceof Error ? failure.message : String(failure) }
  }
</script>
<section aria-label="Видеоэкспорт Filmcraft" class="video-export">
  <FilmcraftExportPresetLibrary {token} disabled={submitting || retrying || disabled} onapply={(name) => libraryPreset = name} />
  <button type="button" disabled={!token || submitting || retrying || disabled} onclick={quickExport}>Быстрый экспорт</button>
  <span>In/Out секвенции или вся секвенция · выбранный сохранённый пресет или последний пресет быстрого экспорта</span>
  {#if quickPresetName}<span>Последний быстрый экспорт: {quickPresetName}</span>{/if}
  {#if !libraryPreset}
  <label>Пресет <select aria-label="Пресет видеоэкспорта Filmcraft" value={preset} onchange={(event) => { preset = event.currentTarget.value as FilmcraftExportPreset; customSize = false; scaling = preset === 'social_vertical' ? 'scaleToFill' : 'scaleToFit' }} disabled={submitting || retrying || disabled}>
    <option value="lossless">MKV · FFV1 без потерь</option>
    <option value="adaptive_high">MP4 · H.264 · адаптивный высокий битрейт</option>
    <option value="adaptive_medium">MP4 · H.264 · адаптивный средний битрейт</option>
    <option value="adaptive_low">MP4 · H.264 · адаптивный низкий битрейт</option>
    <option value="high_quality1080">High Quality 1080p HD · 20 Мбит/с</option>
    <option value="high_quality2160">High Quality 2160p 4K · 60 Мбит/с</option>
    <option value="youtube1080">YouTube 1080p Full HD · 16 Мбит/с</option>
    <option value="youtube2160">YouTube 2160p 4K Ultra HD · 45 Мбит/с</option>
    <option value="vimeo1080">Vimeo 1080p Full HD · 2 прохода</option>
    <option value="vimeo2160">Vimeo 2160p 4K Ultra HD · 2 прохода</option>
    <option value="social_vertical">Social Vertical 1080×1920 · 12 Мбит/с</option>
    <option value="hevc">H.265 · HEVC Main · VideoToolbox</option>
    <option value="mjpeg">QuickTime · Motion JPEG · MOV</option>
    <option value="prores_hq">Apple ProRes 422 HQ · MOV</option>
    <option value="prores_standard">Apple ProRes 422 · MOV</option>
    <option value="prores_lt">Apple ProRes 422 LT · MOV</option>
    <option value="prores_proxy">Apple ProRes 422 Proxy · MOV</option>
    <option value="mxf_atom_dnxhr_hq">MXF OP-Atom DNxHR HQ · набор ZIP</option>
    <option value="wav16">Waveform Audio · WAV 16 бит · 48 кГц</option>
    <option value="wav24">Waveform Audio · WAV 24 бит · 48 кГц</option>
    <option value="aiff16">AIFF 16 бит · 48 кГц</option>
    <option value="gif">Animated GIF · 640×360 · 15 fps</option>
    <option value="png_sequence">PNG Sequence · набор ZIP</option>
    <option value="tiff_sequence">TIFF Sequence · набор ZIP</option>
    <option value="bmp_sequence">BMP Sequence · набор ZIP</option>
    <option value="mxf_h264">MXF OP1a H.264 High</option>
    <option value="mxf_dnxhr_hq">MXF OP1a DNxHR HQ</option>
    <option value="mxf_prores_hq">MXF OP1a ProRes 422 HQ</option>
    <option value="apv42210">APV 422-10 · MOV</option>
    <option value="apv42212">APV 422-12 · MOV</option>
    <option value="dnxhr_hq">Avid DNxHR HQ · MOV</option>
    <option value="dnxhr_sq">Avid DNxHR SQ · MOV</option>
    <option value="dnxhr_lb">Avid DNxHR LB · MOV</option>
  </select></label>
  {#if !libraryPreset && preset === 'mjpeg'}
    <label>Качество Motion JPEG <input aria-label="Качество Motion JPEG" type="text" inputmode="numeric" bind:value={mjpegQuality} disabled={submitting || retrying || disabled} /></label>
    {#if !integerWithin(mjpegQuality, 100)}<span role="alert">Введите целое качество от 1 до 100.</span>{/if}
  {/if}
  {#if nativeAudio && withAudio}
    {#if intraMovie}<label>Аудиокодек <select aria-label="Аудиокодек MOV Filmcraft" value={movieAudioCodec} onchange={(event) => movieAudioCodec = event.currentTarget.value as 'auto' | 'aac' | 'pcm'} disabled={submitting || retrying || disabled}><option value="auto">Авто · PCM</option><option value="pcm">PCM</option><option value="aac">AAC</option></select></label>{/if}
    <label>Каналы <select aria-label={`Каналы аудио ${mxfAudio ? 'MXF' : 'MOV'} Filmcraft`} value={movieAudioChannels} onchange={(event) => movieAudioChannels = Number(event.currentTarget.value)} disabled={submitting || retrying || disabled}><option value={1}>Моно</option><option value={2}>Стерео</option><option value={6}>5.1 · L/R/C/LFE/Ls/Rs</option></select></label>
    <label>Частота аудио <select aria-label={`Частота аудио ${mxfAudio ? 'MXF' : 'MOV'} Filmcraft`} value={movieAudioRate} onchange={(event) => movieAudioRate = Number(event.currentTarget.value)} disabled={submitting || retrying || disabled}>
      <option value={0}>Авто · секвенция {sequenceAudioRate / 1000} кГц</option>
      {#if ![8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000, 128000, 192000].includes(sequenceAudioRate)}<option value={sequenceAudioRate}>Частота секвенции · {sequenceAudioRate / 1000} кГц</option>{/if}
      {#each [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000, 128000, 192000] as rate (rate)}<option value={rate} disabled={!moviePcm && !aacRates.includes(rate)}>{rate / 1000} кГц</option>{/each}
    </select></label>
    {#if moviePcm}
      <label>Разрядность PCM <select aria-label={`Разрядность PCM ${mxfAudio ? 'MXF' : 'MOV'} Filmcraft`} value={movieAudioBits} onchange={(event) => pcmAudioBits = Number(event.currentTarget.value) as 16 | 24} disabled={submitting || retrying || disabled}><option value={16}>16 бит</option><option value={24}>24 бита</option></select></label>
    {:else}
      <label>Битрейт AAC, кбит/с <input type="text" inputmode="numeric" aria-label="Битрейт AAC Filmcraft" bind:value={movieAudioKbps} disabled={submitting || retrying || disabled} /></label>
      {#if !movieAudioValid}<span role="alert">Выберите частоту AAC и битрейт от 32 до 512 кбит/с.</span>{/if}
    {/if}
  {/if}
  {#if audioOnly}
    <label>Каналы PCM <select aria-label="Каналы PCM Filmcraft" value={movieAudioChannels} onchange={(event) => movieAudioChannels = Number(event.currentTarget.value)} disabled={submitting || retrying || disabled}><option value={1}>Моно</option><option value={2}>Стерео</option><option value={6}>5.1 · L/R/C/LFE/Ls/Rs</option></select></label>
    <label>Частота PCM <select aria-label="Частота PCM Filmcraft" value={movieAudioRate} onchange={(event) => movieAudioRate = Number(event.currentTarget.value)} disabled={submitting || retrying || disabled}>
      <option value={0}>Авто · секвенция {sequenceAudioRate / 1000} кГц</option>
      {#each [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000, 128000, 192000] as rate (rate)}<option value={rate}>{rate / 1000} кГц</option>{/each}
    </select></label>
    <label>Разрядность PCM <select aria-label="Разрядность PCM Filmcraft" value={effectivePcmBits} onchange={(event) => pcmAudioBits = Number(event.currentTarget.value) as 16 | 24} disabled={submitting || retrying || disabled}><option value={16}>16 бит</option><option value={24}>24 бита</option></select></label>
  {/if}
  {#if !audioOnly}
  {#if h26x}
    {#if h264Movie}
      <label>Контейнер <select aria-label={`Контейнер ${hevc ? 'HEVC' : 'H264'} Filmcraft`} value={multiplexer} onchange={(event) => { multiplexer = event.currentTarget.value as 'mp4' | 'mov'; if (multiplexer === 'mp4') { if (movieAudioCodec === 'pcm') movieAudioCodec = 'auto'; if (!aacRates.includes(effectiveMovieRate)) movieAudioRate = 48000 } }} disabled={submitting || retrying || disabled}>
        <option value="mp4">MP4</option><option value="mov">QuickTime MOV</option>
      </select></label>
      {#if withAudio}
        <label>Аудиокодек <select aria-label={`Аудиокодек ${hevc ? 'HEVC' : 'H264'} Filmcraft`} value={movieAudioCodec} onchange={(event) => { movieAudioCodec = event.currentTarget.value as 'auto' | 'aac' | 'pcm'; if (movieAudioCodec !== 'pcm' && !aacRates.includes(effectiveMovieRate)) movieAudioRate = 48000 }} disabled={submitting || retrying || disabled}>
          <option value="auto">Авто · AAC</option><option value="aac">AAC</option><option value="pcm" disabled={multiplexer !== 'mov'}>PCM · MOV</option>
        </select></label>
        <label>Каналы <select aria-label={`Каналы аудио ${hevc ? 'HEVC' : 'H264'} Filmcraft`} value={movieAudioChannels} onchange={(event) => movieAudioChannels = Number(event.currentTarget.value)} disabled={submitting || retrying || disabled}><option value={1}>Моно</option><option value={2}>Стерео</option><option value={6}>5.1 · L/R/C/LFE/Ls/Rs</option></select></label>
        <label>Частота аудио <select aria-label={`Частота аудио ${hevc ? 'HEVC' : 'H264'} Filmcraft`} value={movieAudioRate} onchange={(event) => movieAudioRate = Number(event.currentTarget.value)} disabled={submitting || retrying || disabled}>
          <option value={0}>Авто · секвенция {sequenceAudioRate / 1000} кГц</option>
          {#each [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000, 128000, 192000] as rate (rate)}
            <option value={rate} disabled={!moviePcm && rate > 96000}>{rate / 1000} кГц</option>
          {/each}
        </select></label>
        {#if !moviePcm && !aacRates.includes(effectiveMovieRate)}<span role="alert">Частота секвенции недоступна для AAC. Выберите поддерживаемую частоту.</span>{/if}
        {#if moviePcm}
          <label>Разрядность PCM <select aria-label={`Разрядность PCM ${hevc ? 'HEVC' : 'H264'} Filmcraft`} value={movieAudioBits} onchange={(event) => pcmAudioBits = Number(event.currentTarget.value) as 16 | 24} disabled={submitting || retrying || disabled}><option value={16}>16 бит</option><option value={24}>24 бита</option></select></label>
        {:else}
          <label>Битрейт AAC, кбит/с <input type="text" inputmode="numeric" aria-label="Битрейт AAC Filmcraft" bind:value={movieAudioKbps} disabled={submitting || retrying || disabled} /></label>
          {#if !movieAudioValid}<span role="alert">Укажите битрейт AAC от 32 до 512 кбит/с.</span>{/if}
        {/if}
      {/if}
    {/if}
    <label><input type="checkbox" aria-label={`Свой битрейт ${hevc ? 'HEVC' : 'H264'} Filmcraft`} bind:checked={customBitrate} disabled={submitting || retrying || disabled} /> Свой битрейт</label>
    {#if customBitrate}
      <label>Режим битрейта <select aria-label={`Режим битрейта ${hevc ? 'HEVC' : 'H264'} Filmcraft`} value={bitrateMode} onchange={(event) => bitrateMode = event.currentTarget.value as FilmcraftBitrateSettings['mode']} disabled={submitting || retrying || disabled}>
        <option value="cbr">CBR</option><option value="vbr1Pass">VBR · 1 проход</option><option value="vbr2Pass" disabled={hevc}>VBR · 2 прохода</option>
      </select></label>
      <label><input type="checkbox" aria-label={`Адаптивный битрейт ${hevc ? 'HEVC' : 'H264'} Filmcraft`} bind:checked={adaptiveBitrate} disabled={submitting || retrying || disabled} /> Адаптивный битрейт по размеру и частоте кадров</label>
      {#if adaptiveBitrate}
        <label>Бит на пиксель кадра <input type="number" min="0.001" step="0.01" aria-label={`Бит на пиксель ${hevc ? 'HEVC' : 'H264'} Filmcraft`} bind:value={adaptiveBpp} disabled={submitting || retrying || disabled} /></label>
      {:else}
        <label>Целевой, кбит/с <input type="text" inputmode="numeric" aria-label={`Целевой битрейт ${hevc ? 'HEVC' : 'H264'} Filmcraft`} bind:value={bitrateTarget} disabled={submitting || retrying || disabled} /></label>
      {/if}
      {#if bitrateMode !== 'cbr'}
        <label><input type="checkbox" aria-label={`Свой максимум битрейта ${hevc ? 'HEVC' : 'H264'} Filmcraft`} bind:checked={customMaxBitrate} disabled={submitting || retrying || disabled} /> Задать максимальный битрейт</label>
        {#if customMaxBitrate}<label>Максимальный, кбит/с <input type="text" inputmode="numeric" aria-label={`Максимальный битрейт ${hevc ? 'HEVC' : 'H264'} Filmcraft`} bind:value={bitrateMax} disabled={submitting || retrying || disabled} /></label>{/if}
      {/if}
      <span>Минимальный целевой битрейт — 100 кбит/с. Максимум ниже целевого заменяется автоматически на 150% целевого.</span>
      {#if !bitrateValid}<span role="alert">Проверьте битрейт; предел — 1000000 кбит/с.</span>{/if}
    {/if}
    {#if h264}<label>Профиль H.264 <select aria-label={`Профиль ${hevc ? 'HEVC' : 'H264'} Filmcraft`} value={h264Profile} onchange={(event) => h264Profile = event.currentTarget.value as H264Profile} disabled={submitting || retrying || disabled}>
      <option value="baseline">Baseline</option><option value="main">Main</option><option value="high">High</option>
    </select></label>
    {/if}
    <label><input type="checkbox" aria-label="Свой интервал ключевых кадров Filmcraft" bind:checked={customKeyframeDistance} disabled={submitting || retrying || disabled} /> Свой интервал ключевых кадров</label>
    {#if customKeyframeDistance}
      <label>Расстояние в кадрах <input type="text" inputmode="numeric" aria-label="Интервал ключевых кадров Filmcraft" bind:value={keyframeDistance} disabled={submitting || retrying || disabled} /></label>
      {#if !keyframeValid}<span role="alert">Укажите положительное целое число кадров.</span>{/if}
    {/if}
  {/if}
  <label><input type="checkbox" aria-label="Своя частота кадров Filmcraft" bind:checked={customFrameRate} disabled={submitting || retrying || disabled} /> Своя частота кадров</label>
  {#if customFrameRate}
    <label>Числитель частоты <input type="text" inputmode="numeric" aria-label="Числитель частоты кадров Filmcraft" bind:value={rateNumerator} disabled={submitting || retrying || disabled} /></label>
    <label>Знаменатель частоты <input type="text" inputmode="numeric" aria-label="Знаменатель частоты кадров Filmcraft" bind:value={rateDenominator} disabled={submitting || retrying || disabled} /></label>
    <span>Например, 30000 / 1001. Диапазон задаётся в кадрах исходной последовательности.</span>
    {#if !frameRateValid}<span role="alert">Укажите положительную дробь от 1 до 60 кадров/с.</span>{/if}
  {/if}
  <label><input aria-label="Свой размер кадра Filmcraft" type="checkbox" bind:checked={customSize} disabled={submitting || retrying || disabled} /> Свой размер кадра</label>
  {#if customSize}
    <label>Ширина <input aria-label="Ширина экспорта Filmcraft" type="text" inputmode="numeric" bind:value={outputWidth} disabled={submitting || retrying || disabled} /></label>
    <label>Высота <input aria-label="Высота экспорта Filmcraft" type="text" inputmode="numeric" bind:value={outputHeight} disabled={submitting || retrying || disabled} /></label>
  {/if}
  <label>Масштабирование <select aria-label="Масштабирование экспорта Filmcraft" value={scaling} onchange={(event) => { scaling = event.currentTarget.value as FilmcraftScaling }} disabled={submitting || retrying || disabled || (!customSize && !fixedPreset && !customPixelAspect && !maxRenderQuality)}>
    <option value="scaleToFit">Вписать целиком</option>
    <option value="scaleToFill">Заполнить с обрезкой</option>
    <option value="stretchToFill">Растянуть</option>
  </select></label>
  <label><input aria-label="Свой аспект пикселя Filmcraft" type="checkbox" bind:checked={customPixelAspect} disabled={submitting || retrying || disabled} /> Пропорции пикселя</label>
  {#if customPixelAspect}
    <label>Числитель <input aria-label="Числитель аспекта пикселя Filmcraft" type="text" inputmode="numeric" bind:value={pixelNumerator} disabled={submitting || retrying || disabled} /></label>
    <label>Знаменатель <input aria-label="Знаменатель аспекта пикселя Filmcraft" type="text" inputmode="numeric" bind:value={pixelDenominator} disabled={submitting || retrying || disabled} /></label>
  {/if}
  <label><input aria-label="Высокое качество масштабирования Filmcraft" type="checkbox" bind:checked={maxRenderQuality} disabled={submitting || retrying || disabled} /> Высокое качество масштабирования</label>
  <fieldset disabled={submitting || retrying || disabled}>
    <legend>Изображение поверх видео</legend>
    <label><input type="checkbox" aria-label="Изображение экспорта Filmcraft" bind:checked={imageOverlay.enabled} /> Включить</label>
    {#if imageOverlay.enabled}
      <label>Изображение из библиотеки <select aria-label="Файл изображения экспорта Filmcraft" value={imageOverlay.sourceId} onchange={(event) => imageOverlay.sourceId = event.currentTarget.value}>
        <option value="">Выберите изображение</option>
        {#each imageSources as source (source.id)}<option value={source.id}>{source.title || source.originalFilename || source.filename}</option>{/each}
      </select></label>
      {#if imageSources.length === 0}<span>Добавьте изображение в библиотеку.</span>{/if}
      <label>Положение <select aria-label="Изображение: положение Filmcraft" value={imageOverlay.placement} onchange={(event) => imageOverlay.placement = event.currentTarget.value as FilmcraftOverlayPlacement}>{#each placements as [value, label] (value)}<option {value}>{label}</option>{/each}</select></label>
      <label>Смещение X, пиксели <input type="number" step="0.1" aria-label="Изображение: X Filmcraft" bind:value={imageOverlay.offset[0]} /></label>
      <label>Смещение Y, пиксели <input type="number" step="0.1" aria-label="Изображение: Y Filmcraft" bind:value={imageOverlay.offset[1]} /></label>
      <label>Размер, % ширины <input type="number" min="0.5" max="100" step="0.1" aria-label="Изображение: размер Filmcraft" bind:value={imageOverlay.sizePercent} /></label>
      <label>Непрозрачность, % <input type="number" min="0" max="100" step="0.1" aria-label="Изображение: непрозрачность Filmcraft" bind:value={imageOverlay.opacity} /></label>
      {#if !imageOverlayValid}<span role="alert">Выберите изображение и укажите числовые параметры.</span>{/if}
    {/if}
  </fieldset>
  {#each overlayKinds as kind (kind.key)}
    <fieldset disabled={submitting || retrying || disabled}>
      <legend>{kind.label}</legend>
      <label><input type="checkbox" aria-label={`${kind.label} экспорта Filmcraft`} bind:checked={overlays[kind.key].enabled} /> Включить</label>
      {#if overlays[kind.key].enabled}
        <label>{kind.key === 'timecodeOverlay' ? 'Префикс' : 'Текст'} <textarea aria-label={`${kind.label}: текст Filmcraft`} bind:value={overlays[kind.key].text}></textarea></label>
        <label>Положение <select aria-label={`${kind.label}: положение Filmcraft`} value={overlays[kind.key].placement} onchange={(event) => overlays[kind.key].placement = event.currentTarget.value as FilmcraftOverlayPlacement}>{#each placements as [value, label] (value)}<option {value}>{label}</option>{/each}</select></label>
        <label>Смещение X, пиксели <input type="number" step="0.1" aria-label={`${kind.label}: X Filmcraft`} bind:value={overlays[kind.key].offset[0]} /></label>
        <label>Смещение Y, пиксели <input type="number" step="0.1" aria-label={`${kind.label}: Y Filmcraft`} bind:value={overlays[kind.key].offset[1]} /></label>
        <label>Размер, % высоты <input type="number" min="1" max="50" step="0.1" aria-label={`${kind.label}: размер Filmcraft`} bind:value={overlays[kind.key].sizePercent} /></label>
        <label>Непрозрачность, % <input type="number" min="0" max="100" step="0.1" aria-label={`${kind.label}: непрозрачность Filmcraft`} bind:value={overlays[kind.key].opacity} /></label>
      {/if}
    </fieldset>
  {/each}
  {#if !overlaysValid}<span role="alert">Проверьте текст и числовые параметры наложений.</span>{/if}
  <label><input aria-label="Ограничитель яркости экспорта Filmcraft" type="checkbox" bind:checked={limiterEnabled} disabled={submitting || retrying || disabled} /> Ограничитель яркости</label>
  {#if limiterEnabled}
    <label>Нижний порог, % <input aria-label="Нижний порог яркости Filmcraft" type="number" min="0" max="100" step="0.1" bind:value={limiterMin} disabled={submitting || retrying || disabled} /></label>
    <label>Верхний порог, % <input aria-label="Верхний порог яркости Filmcraft" type="number" min="0" max="100" step="0.1" bind:value={limiterMax} disabled={submitting || retrying || disabled} /></label>
    {#if !limiterValid}<p role="alert">Укажите пороги яркости от 0 до 100 %.</p>{/if}
  {/if}
  <details><summary>Метаданные файла</summary>
    {#each metadataFields as field (field.key)}
      <label>{field.label}
        {#if field.multiline}
          <textarea aria-label={`${field.label} файла Filmcraft`} rows="2" maxlength="16384" bind:value={metadata[field.key]} disabled={submitting || retrying || disabled}></textarea>
        {:else}
          <input aria-label={`${field.label} файла Filmcraft`} type="text" maxlength="16384" bind:value={metadata[field.key]} disabled={submitting || retrying || disabled} />
        {/if}
      </label>
    {/each}
    {#if !metadataValid}<p role="alert">Сократите текст метаданных файла.</p>{/if}
  </details>
  {/if}
  <span>{audioOnly ? `PCM ${effectivePcmBits} бит · ${(movieAudioRate || sequenceAudioRate) / 1000} кГц · ${movieAudioChannels === 1 ? 'моно' : movieAudioChannels === 6 ? '5.1' : 'стерео'}` : !withAudio ? 'без звука' : (h264Movie || nativeAudio) ? (moviePcm ? `PCM ${movieAudioBits} бит · ${effectiveMovieRate / 1000} кГц · ${movieAudioChannels === 1 ? 'моно' : movieAudioChannels === 6 ? '5.1' : 'стерео'}` : `AAC ${Math.min(Number(movieAudioKbps), 6 * movieAudioChannels * effectiveMovieRate / 1000)} кбит/с · ${effectiveMovieRate / 1000} кГц · ${movieAudioChannels === 1 ? 'моно' : movieAudioChannels === 6 ? '5.1' : 'стерео'}`) : preset === 'lossless' ? 'стерео PCM 48 кГц' : 'PCM 24 бит · 48 кГц · стерео'}</span>
  {#if !audioOnly}
  <label><input aria-label="Добавить звук в видеоэкспорт Filmcraft" type="checkbox" bind:checked={withAudio} disabled={submitting || retrying || disabled || silentVisual} /> Добавить звук</label>
  {/if}
  {#if withAudio && !silentVisual}
    <label><input aria-label="Нормализация громкости Filmcraft" type="checkbox" bind:checked={loudnessEnabled} disabled={submitting || retrying || disabled} /> Нормализация громкости</label>
    {#if loudnessEnabled}
      <label>Целевая громкость, LUFS <input aria-label="Целевая громкость LUFS Filmcraft" type="number" step="0.1" bind:value={loudnessTarget} disabled={submitting || retrying || disabled} /></label>
      <label>Предел true peak, dBTP <input aria-label="Предел true peak Filmcraft" type="number" min="-24" max="0" step="0.1" bind:value={loudnessPeak} disabled={submitting || retrying || disabled} /></label>
      {#if !loudnessValid}<span role="alert">Укажите числовые значения; предел true peak — от −24 до 0 dBTP.</span>{/if}
    {/if}
  {/if}
  {:else}
    <p>Сохранённый пресет: {libraryPreset}</p>
    <button type="button" disabled={submitting || retrying || disabled} onclick={() => libraryPreset = ''}>Настроить экспорт вручную</button>
  {/if}
  <label>Начало, кадров <input aria-label="Начальный кадр видеоэкспорта Filmcraft" type="text" inputmode="numeric" bind:value={startFrames} disabled={submitting || retrying || disabled} /></label>
  <label>Кадров <input aria-label="Количество кадров видеоэкспорта Filmcraft" type="text" inputmode="numeric" bind:value={frames} disabled={submitting || retrying || disabled} /></label>
  <button type="button" disabled={!token || submitting || retrying || disabled || !valid} onclick={start}>{libraryPreset ? 'Экспортировать с сохранённым пресетом' : audioOnly ? 'Экспортировать аудио' : preset === 'gif' ? 'Экспортировать GIF' : imageSequence ? 'Экспортировать последовательность' : withAudio ? 'Экспортировать видео со звуком' : 'Экспортировать видео без звука'}</button>
  {#if submitting}<span role="status">Добавление экспорта в очередь…</span>{/if}
  {#if busy && jobId}<span>Можно добавить следующий экспорт в очередь</span>{/if}
  {#if !token}<span>Войдите для фонового экспорта</span>{/if}
  {#if busy && jobId}
    <progress aria-label="Прогресс видеоэкспорта Filmcraft" value={progress} max="100"></progress>
    <span>{Math.round(progress)}% {cancelling ? '· отмена запрошена' : ''}</span>
    <button type="button" disabled={cancelling} onclick={cancel}>Отменить видеоэкспорт</button>
    {#if !monitoring}<button type="button" onclick={() => active && observe(active, jobId)}>Проверить видеоэкспорт</button>{/if}
  {/if}
  {#if jobId}<span class="job-id">Задание: {jobId}</span>{/if}
  {#if job?.status === 'done' && job.result && typeof job.result === 'object' && 'loudness' in job.result && job.result.loudness && typeof job.result.loudness === 'object' && 'gainDb' in job.result.loudness && typeof job.result.loudness.gainDb === 'number'}
    <span>Нормализация: усиление {job.result.loudness.gainDb.toFixed(2)} дБ{#if 'measuredLufs' in job.result.loudness && typeof job.result.loudness.measuredLufs === 'number'} · измерено {job.result.loudness.measuredLufs.toFixed(2)} LUFS{/if}</span>
  {/if}
  {#if outputId}<button type="button" disabled={downloading} onclick={download}>{downloading ? 'Скачивание…' : outputFormat === 'zip' ? 'Скачать набор ZIP' : ['wav', 'aiff'].includes(outputFormat) ? `Скачать аудио ${outputFormat.toUpperCase()}` : `Скачать видео ${outputFormat.toUpperCase()}`}</button>{/if}
  {#if outputId && captionSidecar}<button type="button" onclick={downloadCaptions}>Скачать субтитры {captionSidecar.extension.toUpperCase()}</button>{/if}
  {#if canRetry}<button type="button" disabled={submitting || busy || retrying} onclick={retry}>{retrying ? 'Повторный запуск…' : 'Повторить видеоэкспорт'}</button>{/if}
  {#if job?.status === 'cancelled'}<span role="status">Видеоэкспорт отменён</span>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  {#if submitError}<p role="alert">{submitError}</p>{/if}
  {#if token}
    <div class="history" aria-label="История видеоэкспорта Filmcraft">
      <button type="button" disabled={historyBusy} onclick={() => active && loadHistory(active)}>Обновить историю экспорта</button>
      {#if historyBusy}<span role="status">Загрузка истории…</span>{/if}
      {#each history as row (row.jobId)}
        <div class="history-row">
          <span>{new Date(row.createdAt * 1000).toLocaleString()} · {statusText(row.job.status)} · {row.jobId}</span>
          <button type="button" aria-label={`Открыть экспорт ${row.jobId}`} disabled={submitting || retrying || downloading} onclick={() => openHistory(row)}>Открыть</button>
        </div>
      {/each}
      {#if nextCursor}<button type="button" disabled={historyBusy} onclick={() => active && loadHistory(active, true)}>Ещё экспорты</button>{/if}
      {#if !historyBusy && !history.length && !nextCursor && !historyError}<span>История экспорта пуста</span>{/if}
      {#if historyError}<p role="alert">{historyError}</p>{/if}
    </div>
  {/if}
  <FilmcraftExportQueue {token} disabled={disabled || !valid} input={() => { const request = buildRequest(); return { ...request, ...(!request.libraryPreset ? { preset: request.preset ?? 'lossless' } : {}) } }} />
</section>
<style>
  .video-export { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin-block: 0.5rem; font-size: 0.8rem; }
  label { display: flex; align-items: center; gap: 0.25rem; }
  input[type="text"] { width: 8rem; }
  progress { width: 7rem; }
  .job-id { overflow-wrap: anywhere; }
  .history { flex-basis: 100%; display: grid; gap: 0.4rem; }
  .history-row { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; overflow-wrap: anywhere; }
  p { flex-basis: 100%; margin: 0; }
</style>
