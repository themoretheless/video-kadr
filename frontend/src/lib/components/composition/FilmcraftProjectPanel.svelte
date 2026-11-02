<script lang="ts">
  import type { FilmcraftSourceEditFence, FilmcraftLabelGroupSelection, FilmcraftNavigationResult, FilmcraftSourcePlacementResult, FilmcraftSubclipResult } from '$lib/api'
  import { untrack } from 'svelte'
  import { MonitorQueue } from '$lib/filmcraft/monitor-queue'
  import { trimFilmcraftToPlayhead, type FilmcraftTrimShift, trimFilmcraftEditPoints, getFilmcraftNearestEditPoints, type FilmcraftNearestEditKind, getFilmcraftTrimMonitor, type FilmcraftTrimMonitor, applyFilmcraftSelectionTransitions, setFilmcraftTransition, applyFilmcraftTransition, getFilmcraftTransitions, type FilmcraftTransitionCatalogEntry, type FilmcraftTransitionParameter, importFilmcraftProject, exportFilmcraftAudio, exportFilmcraftFrame, trimFilmcraftProject, slipFilmcraftProject, stretchFilmcraftProject, rollFilmcraftProject, slideFilmcraftProject, deleteFilmcraftClips, closeFilmcraftGap, razorFilmcraftProject, removeFilmcraftRange, copyFilmcraftClips, pasteFilmcraftClips, type FilmcraftClipboardResult, type FilmcraftImportResult } from '$lib/api'
  import { parseFilmcraftSession, encodeFilmcraftSession, validTransitionDurationText } from '$lib/filmcraft/session'
  import FilmcraftMonitorFrame from './FilmcraftMonitorFrame.svelte'
  import FilmcraftLabelControls from './FilmcraftLabelControls.svelte'
  import FilmcraftClipEditControls from './FilmcraftClipEditControls.svelte'
  import FilmcraftMediaModifyControls from './FilmcraftMediaModifyControls.svelte'
  import FilmcraftVideoOptionsControls from './FilmcraftVideoOptionsControls.svelte'
  import FilmcraftReplaceControls from './FilmcraftReplaceControls.svelte'
  import FilmcraftAttributesControls from './FilmcraftAttributesControls.svelte'
  import FilmcraftSubclipControls from './FilmcraftSubclipControls.svelte'
  import FilmcraftMarksControls from './FilmcraftMarksControls.svelte'
  import FilmcraftBinActions from './FilmcraftBinActions.svelte'
  import FilmcraftRenameControls from './FilmcraftRenameControls.svelte'
  import FilmcraftScalarParameterEditor from './FilmcraftScalarParameterEditor.svelte'
  import FilmcraftProgramPreview from './FilmcraftProgramPreview.svelte'
  import FilmcraftVideoExportPanel from './FilmcraftVideoExportPanel.svelte'
  import FilmcraftTrimPlaybackControls from './FilmcraftTrimPlaybackControls.svelte'
  import type { MediaEntry } from '$lib/types'
  let { token = null, library = [] }: { token?: string | null; library?: MediaEntry[] } = $props()
  let project = $state<FilmcraftImportResult | null>(null)
  let sourceItem = $state('')
  let sourcePlayhead = $state('0')
  let stillImageDuration = $state('5')
  let stillImageUnit = $state<'seconds' | 'frames'>('seconds')
  let sequenceId = $state('')
  let clipId = $state('')
  const sequence = $derived(project?.sequences.find((s) => s.id === sequenceId))
  let editPoints = $state<{ clipId: string; edge: 'in' | 'out'; kind?: 'trim' | 'ripple' | 'roll' }[]>([])
  let trimShift = $state<FilmcraftTrimShift>({ outShift: '0', inShift: '0' })
  let trimShiftFrames = $state<FilmcraftTrimShift>({ outShift: '0', inShift: '0' })
  function resetTrimShift() { trimShift = { outShift: '0', inShift: '0' }; trimShiftFrames = { outShift: '0', inShift: '0' } }
  let pointTrimFrames = $state('1')
  let pointTrimCount = $state<number | null>(null)
  const validPointTrim = $derived(/^-?\d{1,19}$/.test(pointTrimFrames) && BigInt(pointTrimFrames) >= -9223372036854775808n && BigInt(pointTrimFrames) <= 9223372036854775807n)
  async function trimPoints(extend = false) {
    if (!project || !sequenceId || !editPoints.length || busy || !(extend ? validEditPosition : validPointTrim)) return
    busy = true; error = ''; pointTrimCount = null; controller = new AbortController()
    try {
      const edited = await trimFilmcraftEditPoints({ document: project.documentText, sequenceId, points: editPoints.map((point) => ({ clip: point.clipId, out: point.edge === 'out', kind: point.kind || 'trim' })), linked, ...(extend ? { playheadFrames: editPositionFrames } : { deltaFrames: pointTrimFrames }), shift: trimShift }, controller.signal)
      if (controller.signal.aborted) return
      commitEdit(edited)
      trimShift = edited.shift
      trimShiftFrames = edited.shiftFrames
      if (edited.refusal !== null) error = edited.refusal
      else pointTrimCount = edited.appliedTicks.length
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  async function trimRangeToPlayhead(side: 'previous' | 'next', ripple: boolean) {
    if (!project || !sequenceId || busy || (!pastePositionTicks && !validEditPosition) || (!selectedIds.length && !nearestTargets.length)) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const edited = await trimFilmcraftToPlayhead({ document: project.documentText, sequenceId, clipIds: selectedIds, targetedTrackIds: nearestTargets, side, ripple, linked, ...(pastePositionTicks ? { timeTicks: pastePositionTicks } : { timeFrames: editPositionFrames }) }, controller.signal)
      if (controller.signal.aborted) return
      commitEdit(edited)
      editPositionFrames = edited.playheadFrames; pastePositionTicks = edited.playheadTicks; pastePlayheadTicks = ''
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  let monitorOpen = $state(false)
  let monitor = $state<FilmcraftTrimMonitor | null>(null)
  let monitorLoading = $state(false)
  let monitorError = $state('')
  let monitorRetry = $state(0)
  const monitorQueue = new MonitorQueue(
    (request: { input: Parameters<typeof getFilmcraftTrimMonitor>[0]; token: string | null; retry: number }, signal) => getFilmcraftTrimMonitor(request.input, signal, request.token),
    {
      reset: () => { monitor = null; monitorError = ''; monitorLoading = false },
      result: (result) => { monitor = result; monitorError = ''; monitorLoading = false },
      error: (failure) => { monitorError = failure instanceof Error ? failure.message : String(failure) },
      pending: (pending) => { monitorLoading = pending && !monitor }
    },
    (a, b) => a.input.document === b.input.document && a.retry === b.retry
  )
  $effect(() => {
    const text = project?.documentText; const selectedSequence = sequenceId; const point = editPoints[0]; const retry = monitorRetry
    if (!monitorOpen || !text || !point || !selectedSequence) { untrack(() => monitorQueue.clear()); return }
    void retry
    const sources = Object.fromEntries(Object.entries(activeBindings).filter(([,id])=>id))
    const key = JSON.stringify([selectedSequence, point.clipId, point.edge, point.kind, token, sources])
    const request = { input: { document: text, sequenceId: selectedSequence, clipId: point.clipId, edge: point.edge, includeFrames: true, sources }, token, retry }
    untrack(() => { monitorError = ''; monitorQueue.submit(key, request) })
  })
  $effect(() => () => monitorQueue.clear())
  let nearestKind = $state<FilmcraftNearestEditKind>('roll')
  let nearestTargets = $state<string[]>([])
  const nearestTracks = $derived([...new Map(sequence?.clips?.map((clip) => [clip.trackId, { id: clip.trackId, kind: clip.trackKind }]) || []).values()])
  async function selectNearestPoints() {
    if (!project || !sequenceId || busy || !validEditPosition) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const result = await getFilmcraftNearestEditPoints({ document: project.documentText, sequenceId, kind: nearestKind, targetedTrackIds: nearestTargets, timeFrames: editPositionFrames }, controller.signal)
      if (controller.signal.aborted) return
      resetTrimShift()
      editPoints = result.editPoints.map((point) => ({ clipId: point.clip, edge: point.out ? 'out' : 'in', kind: point.kind }))
      clipId = ''; extraSelectedIds = []; rightClipId = ''; pointTrimCount = null; batchTransitionCount = null
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  let editPointEdge = $state<'in' | 'out'>('out')
  let rightClipId = $state('')
  let extraSelectedIds = $state<string[]>([])
  let projectSelection = $state<string[]>([])
  let projectReveal = $state<{ itemId: string; serial: number } | null>(null)
  const timelineSelection = $derived([clipId, ...extraSelectedIds].filter(id => sequence?.clips?.some(clip => clip.id === id)))
  let edge = $state<'in' | 'out'>('out')
  let trimMode = $state<'regular' | 'ripple'>('regular')
  let trimFrames = $state(1)
  let editPositionFrames = $state('0')
  let pastePositionTicks = $state('')
  const validEditPosition = $derived(/^-?\d{1,19}$/.test(editPositionFrames) && BigInt(editPositionFrames) >= -9223372036854775808n && BigInt(editPositionFrames) <= 9223372036854775807n)
  let rangeDurationFrames = $state(24)
  let linked = $state(true)
  let clipboard = $state<FilmcraftClipboardResult | null>(null)
  let includeMarkers = $state(false)
  let rippleMarkers = $state(false)
  let pastePlayheadTicks = $state('')
  function reconcileSelection(document: FilmcraftImportResult) {
    const ids = document.sequences.find((item) => item.id === sequenceId)?.clips?.map((clip) => clip.id) || []
    if (!ids.includes(clipId)) { clipId = ''; rightClipId = '' }
    editPoints = editPoints.filter((point) => ids.includes(point.clipId))
    extraSelectedIds = extraSelectedIds.filter((id) => ids.includes(id))
    const projectIds = new Set(document.projectItems?.map(item => item.id) ?? [...document.sequences.map(item => item.id), ...document.media.map(item => item.id)])
    projectSelection = projectSelection.filter(id => projectIds.has(id))
    if (sourceItem && !projectIds.has(sourceItem)) sourceItem = ''
    if (renameItemId && !projectIds.has(renameItemId)) renameItemId = ''
    if (renameBinId && !document.projectBins?.some(bin => bin.id === renameBinId)) renameBinId = ''
    if (editingTransitionId && !document.sequences.find((item) => item.id === sequenceId)?.transitions?.some((item) => item.id === editingTransitionId)) { editingTransitionId = ''; transitionDraft = {} }
  }
  let renameBinId = $state('')
  const renameBins = $derived(project?.projectBins?.filter(bin => bin.parentId !== null) ?? [])
  let renameItemId = $state('')
  const projectItemLabels = { Media: 'Медиа', Sequence: 'Секвенция', Subclip: 'Подклип', Graphic: 'Графика', AdjustmentLayer: 'Корректирующий слой' }
  const renameItems = $derived(project ? project.projectItems?.map(item => ({ ...item, kind: projectItemLabels[item.kind] })) ?? [...project.sequences.map(item => ({ ...item, kind: 'Секвенция' })), ...project.media.map(item => ({ ...item, kind: 'Медиа' }))] : [])
  function acceptsFence(expected?: FilmcraftSourceEditFence) {
    if (expected && (project?.documentText !== expected.document || sequenceId !== expected.sequenceId || clipId !== expected.clipId || token !== expected.token)) return false
    if (expected?.binId !== undefined && (expected.binId !== renameBinId || !renameBins.some(bin => bin.id === expected.binId))) return false
    if (expected?.itemId !== undefined && (expected.itemId !== renameItemId || !renameItems.some(item => item.id === expected.itemId))) return false
    if (expected?.sourceItem !== undefined && expected.sourceItem !== (sourceItem || null)) return false
    if (expected?.sourcePlayhead !== undefined && expected.sourcePlayhead !== sourcePlayhead) return false
    if (expected?.sourceBindings && (Object.keys(expected.sourceBindings).length !== Object.keys(activeBindings).length || Object.entries(expected.sourceBindings).some(([id, source]) => activeBindings[id] !== source))) return false
    if (expected?.timeFrames !== undefined && expected.timeFrames !== editPositionFrames) return false
    if (expected?.timeTicks !== undefined && expected.timeTicks !== (pastePositionTicks || null)) return false
    if (expected?.projectSelection && JSON.stringify(expected.projectSelection) !== JSON.stringify(projectSelection)) return false
    if (expected?.timelineSelection && JSON.stringify(expected.timelineSelection) !== JSON.stringify(timelineSelection)) return false
    return true
  }
  function adoptLabelSelection(result: FilmcraftLabelGroupSelection, expected: FilmcraftSourceEditFence) {
    if (!acceptsFence(expected)) return false
    if ('clips' in result) {
      if (result.clips.some(id => !sequence?.clips?.some(clip => clip.id === id))) return false
      clipId = result.clips[0] || ''; extraSelectedIds = result.clips.slice(1); rightClipId = ''; editPoints = []
    } else {
      const ids = new Set(project?.projectItems?.map(item => item.id) ?? [])
      if (result.items.some(id => !ids.has(id))) return false
      projectSelection = [...result.items]
    }
    return true
  }
  function adoptNavigation(result: FilmcraftNavigationResult, expected: FilmcraftSourceEditFence) {
    if (!acceptsFence(expected) || result.context.activeSequence !== (sequenceId || null)) return false
    const fixed = result.context
    if (fixed.sourceItem !== null && fixed.sourceItem !== (sourceItem || null) && !project?.projectItems?.some(item => item.id === fixed.sourceItem) && !result.events.some(event => event.type === 'openSource' && event.item === fixed.sourceItem) || fixed.selection.some(id => !sequence?.clips?.some(clip => clip.id === id))) return false
    const projectIds = new Set(project?.projectItems?.map(item => item.id) ?? [...(project?.sequences.map(item => item.id) ?? []), ...(project?.media.map(item => item.id) ?? [])])
    if (fixed.projectSelection.some(id => !projectIds.has(id))) return false
    projectSelection = [...fixed.projectSelection]
    const reveal = result.events.find(event => event.type === 'revealInProject')
    if (reveal) projectReveal = { itemId: reveal.item, serial: (projectReveal?.serial ?? 0) + 1 }
    sourceItem = fixed.sourceItem || ''; sourcePlayhead = fixed.sourcePlayhead
    editPositionFrames = fixed.playheadFrames; pastePositionTicks = fixed.playhead; pastePlayheadTicks = ''
    if (JSON.stringify(fixed.selection) !== JSON.stringify(timelineSelection)) { clipId = fixed.selection[0] || ''; extraSelectedIds = fixed.selection.slice(1); rightClipId = ''; editPoints = [] }
    return true
  }
  function commitEdit(edited: FilmcraftImportResult, expected?: FilmcraftSourceEditFence) {
    if (!acceptsFence(expected)) return false
    const subclip = 'subclipEdit' in edited ? edited as FilmcraftSubclipResult : null
    if (subclip && (!edited.projectItems?.some(item => item.id === subclip.subclipEdit.itemId) || subclip.subclipProjectSelection.some(id => !edited.projectItems?.some(item => item.id === id)))) return false
    const placement = 'sourceEditContext' in edited ? (edited as FilmcraftSourcePlacementResult) : null
    if (placement && (placement.sourceEdit !== true || placement.sourceEditContext.activeSequence !== sequenceId || placement.sourceEditContext.sourceItem !== sourceItem || placement.sourceEditContext.selection.some(id => !placement.insertedClipIds.includes(id) && !edited.sequences.find(sequence => sequence.id === sequenceId)?.clips?.some(clip => clip.id === id)))) return false
    const deletion = 'deletionContext' in edited ? edited.deletionContext as { activeSequence: string | null; selection: string[]; projectSelection: string[] } : null
    if (deletion && (deletion.activeSequence !== null && !edited.sequences.some(sequence => sequence.id === deletion.activeSequence) || deletion.projectSelection.length || deletion.selection.some(id => !edited.sequences.find(sequence => sequence.id === deletion.activeSequence)?.clips?.some(clip => clip.id === id)))) return false
    const clipEditSelection = 'clipEditSelection' in edited && Array.isArray(edited.clipEditSelection) ? edited.clipEditSelection as string[] : undefined
    if (clipEditSelection?.some(id => !edited.sequences.find(sequence => sequence.id === sequenceId)?.clips?.some(clip => clip.id === id))) return false
    // Original Duplicate and Clear push an edit frame even for an accepted no-op.
    if (project && (edited.documentText !== project.documentText || 'duplicatedItemIds' in edited && Array.isArray(edited.duplicatedItemIds) || 'deletedItemIds' in edited && Array.isArray(edited.deletedItemIds) || 'clipEdit' in edited && edited.clipEdit === true || 'modifyEdit' in edited && edited.modifyEdit === true || 'videoOptionsEdit' in edited && edited.videoOptionsEdit === true || 'replacementEdit' in edited && edited.replacementEdit === true || 'attributeEdit' in edited && edited.attributeEdit === true || 'markEdit' in edited && edited.markEdit === true || placement?.sourceEdit === true || !!subclip)) { undo = boundedHistory([...undo, project]); redo = [] }
    if ('duplicateSources' in edited && Array.isArray(edited.duplicateSources)) {
      const inherited = { ...bindings }
      for (const record of edited.duplicateSources) if (edited.media.some(media => media.id === record.id) && inherited[record.sourceId]) inherited[record.id] = inherited[record.sourceId]
      bindings = inherited
    }
    if ('modifiedProjectSelection' in edited && Array.isArray(edited.modifiedProjectSelection)) projectSelection = [...edited.modifiedProjectSelection]
    if (subclip) {
      const info = subclip.subclipEdit
      if (info.converted && info.bindingSourceId && edited.media.some(media => media.id === info.itemId) && bindings[info.bindingSourceId]) bindings = { ...bindings, [info.itemId]: bindings[info.bindingSourceId] }
      projectSelection = [...subclip.subclipProjectSelection]
    }
    if (placement) {
      const fixed = placement.sourceEditContext
      sourcePlayhead = fixed.sourcePlayhead; editPositionFrames = fixed.playheadFrames; pastePositionTicks = fixed.playhead; pastePlayheadTicks = ''
      clipId = fixed.selection[0] || ''; extraSelectedIds = fixed.selection.slice(1); rightClipId = ''; editPoints = []
    }
    if (deletion) {
      if (sequenceId !== (deletion.activeSequence || '')) { const next = edited.sequences.find(sequence => sequence.id === deletion.activeSequence); start = 0; end = Math.min(next?.durationSeconds || 1, 4194304 / (next?.sampleRate || 48000)); nearestTargets = []; editPositionFrames = '0'; pastePositionTicks = ''; pastePlayheadTicks = ''; trimShift = { outShift: '0', inShift: '0' }; trimShiftFrames = { outShift: '0', inShift: '0' } }
      sequenceId = deletion.activeSequence || ''; clipId = deletion.selection[0] || ''; extraSelectedIds = deletion.selection.slice(1); projectSelection = []; rightClipId = ''; editPoints = []
    }
    if (clipEditSelection) { clipId = clipEditSelection[0] || ''; extraSelectedIds = clipEditSelection.slice(1); rightClipId = ''; editPoints = [] }
    clearDownload(); pointTrimCount = null; batchTransitionCount = null; project = edited; reconcileSelection(edited)
    return true
  }
  async function copySelection() {
    if (!project || !selectedIds.length || busy) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const copied = await copyFilmcraftClips({ document: project.documentText, sequenceId, clipIds: selectedIds, linked, includeMarkers }, controller.signal)
      if (!controller.signal.aborted) clipboard = copied
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  async function pasteSelection(insert: boolean) {
    if (!project || !clipboard || !sequenceId || (!pastePositionTicks && !validEditPosition) || busy) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const edited = await pasteFilmcraftClips({ document: project.documentText, sequenceId, clipboardText: clipboard.clipboardText, insert, includeMarkers, rippleMarkers, ...(pastePositionTicks ? { timeTicks: pastePositionTicks } : { timeFrames: editPositionFrames }) }, controller.signal)
      if (controller.signal.aborted) return
      commitEdit(edited)
      const ids = edited.selectedClipIds.filter((id) => edited.sequences.find((item) => item.id === sequenceId)?.clips?.some((clip) => clip.id === id))
      clipId = ids[0] || ''; extraSelectedIds = ids.slice(1); rightClipId = ''; pastePlayheadTicks = edited.playheadTicks; pastePositionTicks = edited.playheadTicks; editPositionFrames = edited.playheadFrames
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  function pasteTime(ticks: string) {
    try { const time = BigInt(ticks); const absolute = time < 0n ? -time : time; return `${time < 0n ? '-' : ''}${absolute / 254016000000n}.${String(absolute % 254016000000n * 1000n / 254016000000n).padStart(3, '0')}` }
    catch { return '' }
  }
  let undo = $state<FilmcraftImportResult[]>([])
  let redo = $state<FilmcraftImportResult[]>([])
  function boundedHistory(items: FilmcraftImportResult[]) {
    let bytes = 0
    return items.slice(-32).reverse().filter((item) => { bytes += item.documentText.length * 2; return bytes <= 64 * 1024 * 1024 }).reverse()
  }
  function restore(direction: 'undo' | 'redo') {
    if (!project || busy) return
    const history = direction === 'undo' ? undo : redo
    const previous = history.at(-1)
    if (!previous) return
    if (direction === 'undo') { redo = boundedHistory([...redo, project]); undo = undo.slice(0, -1) }
    else { undo = boundedHistory([...undo, project]); redo = redo.slice(0, -1) }
    clearDownload(); project = previous; reconcileSelection(previous); pointTrimCount = null; batchTransitionCount = null; transitionDraft = {}; transitionReverse = previous.sequences.find((item) => item.id === sequenceId)?.transitions?.find((item) => item.id === editingTransitionId)?.reverse || false; error = ''; pastePlayheadTicks = ''; pastePositionTicks = ''
  }
  async function editClip(operation: 'trim' | 'slip' | 'stretch' | 'roll' | 'slide' | 'delete' | 'ripple-delete' | 'close-gap' | 'razor' | 'razor-all' | 'lift-range' | 'extract-range' = 'trim') {
    const range = operation === 'lift-range' || operation === 'extract-range'
    const deleting = operation === 'delete' || operation === 'ripple-delete'
    const absolute = range || operation === 'close-gap' || operation === 'razor' || operation === 'razor-all'
    if (!project || (!clipId && operation !== 'razor-all') || (!deleting && !(absolute ? validEditPosition : Number.isSafeInteger(trimFrames))) || busy) return
    if (range && (!Number.isSafeInteger(rangeDurationFrames) || rangeDurationFrames < 0)) return
    if (operation === 'roll' && !rightClips.some((clip) => clip.id === rightClipId)) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const input = { document: project.documentText, sequenceId, clipId, deltaFrames: String(trimFrames) }
      let edited: FilmcraftImportResult
      switch (operation) {
        case 'delete': case 'ripple-delete': edited = await deleteFilmcraftClips({ document: input.document, sequenceId, clipIds: selectedIds, ripple: operation === 'ripple-delete' }, controller.signal); break
        case 'close-gap': {
          const trackId = project.sequences.find((item) => item.id === sequenceId)?.clips?.find((item) => item.id === clipId)?.trackId
          if (!trackId) return
          edited = await closeFilmcraftGap({ document: input.document, sequenceId, trackId, timeFrames: String(editPositionFrames) }, controller.signal); break
        }
        case 'razor': case 'razor-all': edited = await razorFilmcraftProject({ document: input.document, sequenceId, timeFrames: String(editPositionFrames), ...(operation === 'razor-all' ? { trackIds: [] } : { clipIds: selectedIds }) }, controller.signal); break
        case 'lift-range': case 'extract-range': {
          const trackIds = [...new Set(project.sequences.find((item) => item.id === sequenceId)?.clips?.filter((item) => selectedIds.includes(item.id)).map((item) => item.trackId) || [])]
          edited = await removeFilmcraftRange({ document: input.document, sequenceId, trackIds, startFrames: String(editPositionFrames), durationFrames: String(rangeDurationFrames), ripple: operation === 'extract-range' }, controller.signal); break
        }
        case 'slide': edited = await slideFilmcraftProject(input, controller.signal); break
        case 'slip': edited = await slipFilmcraftProject(input, controller.signal); break
        case 'stretch': edited = await stretchFilmcraftProject({ ...input, edge }, controller.signal); break
        case 'roll': edited = await rollFilmcraftProject({ document: input.document, sequenceId, leftClipId: clipId, rightClipId, deltaFrames: input.deltaFrames }, controller.signal); break
        default: edited = await trimFilmcraftProject({ ...input, edge, mode: trimMode, linked }, controller.signal)
      }
      if (controller.signal.aborted) return
      commitEdit(edited)
      if (deleting) extraSelectedIds = []
      if (!edited.sequences.find((item) => item.id === sequenceId)?.clips?.some((item) => item.id === clipId)) { clipId = ''; rightClipId = '' }
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  let batchVideoEffect = $state('cross_dissolve')
  let batchAudioEffect = $state('constant_power')
  let batchVideoValue = $state('30')
  let batchAudioValue = $state('1')
  let batchVideoUnit = $state<'frames' | 'seconds'>('frames')
  let batchAudioUnit = $state<'frames' | 'seconds'>('seconds')
  let batchTransitionCount = $state<number | null>(null)
  function positiveFrameCount(value: string) { return /^\d{1,19}$/.test(value) && BigInt(value) > 0n && BigInt(value) <= 9223372036854775807n }
  const validBatchTransitions = $derived(validTransitionDurationText(batchVideoValue) && validTransitionDurationText(batchAudioValue))
  function durationInput(value: string, unit: 'frames' | 'seconds'): { frames: string; duration?: never } | { frames?: never; duration: { value: number; unit: 'frames' | 'seconds' } } {
    return unit === 'frames' && positiveFrameCount(value) ? { frames: value } : { duration: { value: Number(value), unit } }
  }
  async function applySelectionTransitions(pointsOnly = false) {
    if (!project || !sequenceId || !(pointsOnly ? editPoints.length : selectedIds.length) || busy || !validBatchTransitions) return
    busy = true; error = ''; batchTransitionCount = null; controller = new AbortController()
    try {
      const video = durationInput(batchVideoValue, batchVideoUnit); const audio = durationInput(batchAudioValue, batchAudioUnit)
      const edited = await applyFilmcraftSelectionTransitions({ document: project.documentText, sequenceId, clipIds: pointsOnly ? [] : selectedIds, ...(pointsOnly ? { editPoints: editPoints.map(({ clipId, edge }) => ({ clipId, edge })) } : {}), videoEffect: batchVideoEffect, audioEffect: batchAudioEffect, ...(video.frames !== undefined ? { videoFrames: video.frames } : { videoDuration: video.duration }), ...(audio.frames !== undefined ? { audioFrames: audio.frames } : { audioDuration: audio.duration }) }, controller.signal)
      if (!controller.signal.aborted) { commitEdit(edited); batchTransitionCount = edited.createdTransitionIds.length }
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  let transitions = $state<FilmcraftTransitionCatalogEntry[]>([])
  let transitionsLoading = $state(false)
  let transitionsError = $state('')
  let transitionController: AbortController | undefined
  let editingTransitionId = $state('')
  let transitionId = $state('')
  let transitionTarget = $state<'edge' | 'playhead'>('edge')
  let transitionFrames = $state('24')
  let transitionUnit = $state<'frames' | 'seconds'>('frames')
  let transitionReverse = $state(false)
  let transitionDraft = $state<Record<string, string>>({})
  const transitionClip = $derived(sequence?.clips?.find((clip) => clip.id === clipId))
  const transitionKind = $derived(transitionClip?.trackKind === 'audio_tracks' ? 'audio' : 'video')
  const transitionTargetTracks = $derived([...new Set(sequence?.clips?.filter((clip) => selectedIds.includes(clip.id) && clip.trackKind === (transitionKind === 'audio' ? 'audio_tracks' : 'video_tracks')).map((clip) => clip.trackId) || [])])
  const validTransitionTarget = $derived(transitionTarget === 'edge' ? !!transitionClip && !transitionClip.locked : transitionTargetTracks.length > 0)
  const availableTransitions = $derived(transitions.filter((entry) => entry.definition.kind === (transitionClip?.trackKind === 'audio_tracks' ? 'AudioTransition' : 'VideoTransition')))
  const editingTransition = $derived(sequence?.transitions?.find((item) => item.id === editingTransitionId))
  const editingParams = $derived.by(() => { try { return JSON.parse(editingTransition?.paramsText || '{}') as Record<string, { value?: Record<string, unknown> }> } catch { return {} } })
  const transition = $derived(editingTransition ? transitions.find((entry) => entry.definition.id === editingTransition.effect)?.definition : availableTransitions.find((entry) => entry.definition.id === transitionId)?.definition || availableTransitions.find((entry) => entry.definition.id === (transitionClip?.trackKind === 'audio_tracks' ? 'constant_power' : 'cross_dissolve'))?.definition || availableTransitions[0]?.definition)
  const validTransitionFrames = $derived(validTransitionDurationText(transitionFrames))
  async function loadTransitions() {
    if (transitions.length || transitionsLoading) return
    transitionsLoading = true; transitionsError = ''; transitionController = new AbortController()
    try { const result = await getFilmcraftTransitions(transitionController.signal); if (!transitionController.signal.aborted) transitions = result }
    catch (failure) { if (!transitionController.signal.aborted) transitionsError = failure instanceof Error ? failure.message : String(failure) }
    finally { transitionsLoading = false }
  }
  function parameterValue(param: FilmcraftTransitionParameter): string {
    const key = parameterKey(param)
    if (key in transitionDraft) return transitionDraft[key]!
    const value = Object.values(editingParams[param.id]?.value || param.default)[0]
    if ('Color' in param.default) { const color = value as number[]; return '#' + color.map((c) => Math.round(Math.max(0, Math.min(1, c)) * 255).toString(16).padStart(2, '0')).join('') }
    if ('Vec2' in param.default) { const point = value as { x: number | null; y: number | null }; return point.x === null || point.y === null ? '' : `${point.x}, ${point.y}` }
    return String(value)
  }
  function parameterChoices(param: FilmcraftTransitionParameter): string[] { return typeof param.kind === 'object' && Array.isArray(param.kind.Choice) ? param.kind.Choice as string[] : [] }
  function parameterKey(param: FilmcraftTransitionParameter) { return `${editingTransition?.id || 'new'}:${transition?.id}:${param.id}` }
  function setParameter(param: FilmcraftTransitionParameter, value: string) { transitionDraft = { ...transitionDraft, [parameterKey(param)]: value } }
  function transitionParams(): Record<string, unknown> {
    const params: Record<string, unknown> = {}
    for (const param of transition?.params || []) {
      const key = parameterKey(param)
      if (!(key in transitionDraft)) continue
      const value = transitionDraft[key]!
      if ('Bool' in param.default) params[param.id] = value === 'true'
      else if ('Color' in param.default) params[param.id] = value
      else if ('Vec2' in param.default) { const point = value.split(',').map((v) => v.trim()); if (point.length !== 2 || point.some((v) => !v || !Number.isFinite(Number(v)))) throw new Error(`${param.label}: укажите X, Y`); params[param.id] = point.map(Number) }
      else { if (!value.trim() || !Number.isFinite(Number(value))) throw new Error(`${param.label}: укажите число`); params[param.id] = Number(value) }
    }
    return params
  }
  function chooseAppliedTransition(id: string) {
    editingTransitionId = id; transitionDraft = {}
    transitionReverse = sequence?.transitions?.find((item) => item.id === id)?.reverse || false
  }
  async function updateTransition(reset = false) {
    if (!project || !editingTransition || busy) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const edited = await setFilmcraftTransition({ document: project.documentText, sequenceId, transitionId: editingTransition.id, ...(reset ? { reset: true } : { params: transitionParams(), reverse: transitionReverse }) }, controller.signal)
      if (!controller.signal.aborted) { commitEdit(edited); transitionDraft = {}; transitionReverse = edited.sequences.find((item) => item.id === sequenceId)?.transitions?.find((item) => item.id === editingTransitionId)?.reverse || false }
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  async function applyTransition() {
    if (!project || !transition || !validTransitionTarget || busy || !validTransitionFrames || !validEditPosition) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const edited = await applyFilmcraftTransition({ document: project.documentText, sequenceId, kind: transitionKind, effect: transition.id, ...durationInput(transitionFrames, transitionUnit), ...(transitionTarget === 'edge' ? { clipId, edge } : { targetedTrackIds: transitionTargetTracks }), timeFrames: editPositionFrames, params: transitionParams(), reverse: transitionReverse }, controller.signal)
      if (!controller.signal.aborted) commitEdit(edited)
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  // Retain inactive choices for document Redo, but never send absent source IDs.
  let bindings = $state<Record<string, string>>({})
  const activeBindings = $derived(Object.fromEntries(Object.entries(bindings).filter(([id]) => project?.media.some(media => media.id === id))))
  let start = $state(0)
  let end = $state(1)
  let format = $state<'wav' | 'aiff'>('wav')
  let bits = $state<16 | 24>(24)
  let channels = $state<1 | 2>(2)
  let busy = $state(false)
  let error = $state('')
  let download = $state('')
  let controller: AbortController | undefined
  const savedFiles = new Map<string, ReturnType<typeof setTimeout>>()
  const selectedIds = $derived.by(() => {
    const clips = sequence?.clips || []
    const selected = new Set([clipId, ...extraSelectedIds].filter((id) => clips.some((clip) => clip.id === id)))
    const links = new Set(clips.filter((clip) => selected.has(clip.id) && clip.linkId).map((clip) => clip.linkId))
    if (linked) for (const clip of clips) if (clip.linkId && links.has(clip.linkId)) selected.add(clip.id)
    return [...selected]
  })
  const rightClips = $derived.by(() => {
    const left = sequence?.clips?.find((clip) => clip.id === clipId)
    if (!left) return []
    try {
      const cut = BigInt(left.startTicks) + BigInt(left.durationTicks)
      return sequence?.clips?.filter((clip) => clip.id !== left.id && clip.trackId === left.trackId && clip.trackKind === left.trackKind && BigInt(clip.startTicks) === cut) || []
    } catch { return [] }
  })
  const files = $derived(library.filter((entry) => entry.kind === 'source' && (entry.mediaType === 'video' || entry.mediaType === 'image' || entry.mediaType === 'audio' || /\.(wav|png|jpe?g|webp|gif|bmp|tiff?|mp4|mov|mkv|webm|avi|mxf|mts|m2ts|mpg|mpeg|flv|ogv)$/i.test(entry.filename))))
  const valid = $derived(sequence && Number.isFinite(start) && Number.isFinite(end) && start >= 0 && end > start && Math.floor(end * sequence.sampleRate) - Math.floor(start * sequence.sampleRate) <= 4194304)
  $effect(() => () => { controller?.abort(); transitionController?.abort(); if (download) URL.revokeObjectURL(download); for (const [url, timer] of savedFiles) { clearTimeout(timer); URL.revokeObjectURL(url) } savedFiles.clear() })
  function clearDownload() { if (download) URL.revokeObjectURL(download); download = '' }
  function chooseSequence() { resetTrimShift(); pointTrimCount = null; nearestTargets = []; editPoints = []; batchTransitionCount = null; editingTransitionId = ''; transitionDraft = {}; pastePositionTicks = ''; pastePlayheadTicks = ''; editPositionFrames = '0'; extraSelectedIds = []; rightClipId = ''; clipId = ''; clearDownload(); start = 0; end = Math.min(sequence?.durationSeconds || 1, 4194304 / (sequence?.sampleRate || 48000)) }
  async function openFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    if (!file || busy) return
    if (file.size > 24 * 1024 * 1024) { error = 'Проект слишком большой для открытия'; input.value = ''; return }
    busy = true; error = ''; controller?.abort(); controller = new AbortController()
    try {
      const text = await file.text()
      const session = parseFilmcraftSession(text)
      const loaded = await importFilmcraftProject(session?.documentText || text, controller.signal)
      if (controller.signal.aborted) return
      if (session && !loaded.sequences.some((item) => item.id === session.sequenceId)) throw new Error('Секвенция из сессии отсутствует в проекте')
      clearDownload(); resetTrimShift(); pointTrimCount = null; nearestTargets = []; editPoints = []; batchTransitionCount = null; editingTransitionId = ''; transitionDraft = {}; transitionTarget = 'edge'; clipboard = null; pastePlayheadTicks = ''; pastePositionTicks = ''; editPositionFrames = '0'; undo = []; redo = []; extraSelectedIds = []; rightClipId = ''; clipId = ''; renameItemId = ''; renameBinId = ''; projectSelection = []; projectReveal = null; sourceItem = ''; sourcePlayhead = '0'; project = loaded; sequenceId = loaded.sequences[0]?.id || ''; bindings = {}
      for (const media of loaded.media.filter((media) => !media.offline)) {
        const name = media.path.split(/[\\/]/).pop()
        const matches = files.filter((entry) => (entry.originalFilename ?? entry.filename) === name)
        if (matches.length === 1) bindings[media.id] = matches[0]!.id
      }
      const settings = session?.transitionSettings
      batchVideoEffect = settings?.video.effect || 'cross_dissolve'; batchAudioEffect = settings?.audio.effect || 'constant_power'; batchVideoValue = settings?.video.duration.value || '30'; batchVideoUnit = settings?.video.duration.unit || 'frames'; batchAudioValue = settings?.audio.duration.value || '1'; batchAudioUnit = settings?.audio.duration.unit || 'seconds'; transitionFrames = settings?.single.value || '24'; transitionUnit = settings?.single.unit || 'frames'
      start = 0; end = Math.min(loaded.sequences[0]?.durationSeconds || 1, 4194304 / (loaded.sequences[0]?.sampleRate || 48000))
      if (session) {
        if (loaded.sequences.some((item) => item.id === session.sequenceId)) sequenceId = session.sequenceId
        bindings = Object.fromEntries(Object.entries(session.sources).filter(([id]) => loaded.media.some((media) => media.id === id)))
        start = session.start; end = session.end; format = session.audioFormat; bits = session.bits; channels = session.channels
      }
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false; input.value = '' }
  }
  function saveText(text: string, filename: string) {
    saveBlob(new Blob([text], { type: 'application/json' }), filename)
  }
  function saveBlob(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a'); anchor.href = url; anchor.download = filename
    document.body.append(anchor); anchor.click(); anchor.remove()
    savedFiles.set(url, setTimeout(() => { URL.revokeObjectURL(url); savedFiles.delete(url) }, 1000))
  }
  function saveSession() {
    if (!project || !valid) return
    try {
      const text = encodeFilmcraftSession({ format: 'video-kadr.filmcraft-session', version: 1, documentText: project.documentText, sequenceId, sources: Object.fromEntries(Object.entries(activeBindings).filter(([, id]) => id)), start, end, audioFormat: format, bits, channels, transitionSettings: { video: { effect: batchVideoEffect, duration: { value: batchVideoValue, unit: batchVideoUnit } }, audio: { effect: batchAudioEffect, duration: { value: batchAudioValue, unit: batchAudioUnit } }, single: { value: transitionFrames, unit: transitionUnit } } })
      if (new Blob([text]).size > 24 * 1024 * 1024) throw new Error('Сессия слишком большая: скачайте исходный проект отдельно')
      saveText(text, 'project.filmcraft-session.json')
    } catch (failure) { error = failure instanceof Error ? failure.message : String(failure) }
  }
  async function exportFrame() {
    if (!project || !sequence || busy) return
    busy = true; error = ''; controller = new AbortController()
    try {
      const blob = await exportFilmcraftFrame({ document: project.documentText, sequenceId, timeFrames: editPositionFrames, sources: Object.fromEntries(Object.entries(activeBindings).filter(([, id]) => id)) }, token, controller.signal)
      if (!controller.signal.aborted) saveBlob(blob, 'frame.png')
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
  async function exportAudio() {
    if (!project || !sequence || !valid || busy) return
    busy = true; error = ''; clearDownload(); controller = new AbortController()
    try {
      const sources = Object.fromEntries(Object.entries(activeBindings).filter(([, id]) => id && !library.some(file => file.id === id && !/\.wav$/i.test(file.filename))))
      const blob = await exportFilmcraftAudio({ document: project.documentText, sequenceId, startSample: Math.floor(start * sequence.sampleRate), frames: Math.floor(end * sequence.sampleRate) - Math.floor(start * sequence.sampleRate), channels, bits, format, sources }, token, controller.signal)
      if (!controller.signal.aborted) download = URL.createObjectURL(blob)
    } catch (failure) { if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure) }
    finally { busy = false }
  }
</script>
<details class="filmcraft-panel">
  <summary>Проект Filmcraft</summary>
  <div class="controls">
    <label>Открыть проект <input aria-label="Открыть проект Filmcraft" type="file" accept=".json,.filmcraft" disabled={busy} onchange={openFile} /></label>
    {#if project}
      <label>Секвенция <select aria-label="Секвенция Filmcraft" value={sequenceId} onchange={(event) => { sequenceId = event.currentTarget.value; chooseSequence() }} disabled={busy}>{#each project.sequences as item}<option value={item.id}>{item.name || item.id}</option>{/each}</select></label>
      {#each project.media.filter((media) => !media.offline) as media}
        <label>{media.name || media.path}<select aria-label={`Файл для ${media.name || media.id}`} value={bindings[media.id] || ''} disabled={busy} onchange={(event) => { bindings = { ...bindings, [media.id]: event.currentTarget.value }; clearDownload() }}><option value="">Выбрать файл из библиотеки</option>{#if bindings[media.id] && !files.some((file) => file.id === bindings[media.id])}<option value={bindings[media.id]}>Файл недоступен — выберите замену</option>{/if}{#each files as file}<option value={file.id}>{file.title || file.filename}</option>{/each}</select></label>
      {/each}
      <label>От, с <input type="number" min="0" step="0.001" bind:value={start} disabled={busy} onchange={clearDownload} /></label>
      <label>До, с <input type="number" min="0" step="0.001" bind:value={end} disabled={busy} onchange={clearDownload} /></label>
      <label>Формат <select bind:value={format} disabled={busy} onchange={clearDownload}><option value="wav">WAV</option><option value="aiff">AIFF</option></select></label>
      <label>Разрядность <select bind:value={bits} disabled={busy} onchange={clearDownload}><option value={16}>16 бит</option><option value={24}>24 бита</option></select></label>
      <label>Каналы <select bind:value={channels} disabled={busy} onchange={clearDownload}><option value={1}>Моно</option><option value={2}>Стерео</option></select></label>
      <button type="button" disabled={busy || !valid} onclick={exportAudio}>{busy ? 'Подготовка…' : 'Экспортировать звук'}</button>
      <button type="button" disabled={busy || !sequence} onclick={exportFrame}>Экспортировать кадр PNG</button>
      <button type="button" disabled={busy} onclick={() => project && saveText(project.documentText, 'project.filmcraft.json')}>Скачать проект</button>
      <button type="button" disabled={busy || !valid} onclick={saveSession}>Сохранить рабочую сессию</button>
        <label>Позиция правки, кадров <input aria-label="Позиция правки, кадров" type="text" inputmode="numeric" value={editPositionFrames} oninput={(event) => { editPositionFrames = event.currentTarget.value; pastePositionTicks = '' }} disabled={busy} /></label>
      <FilmcraftLabelControls document={project.documentText} {sequenceId} {clipId} {token} items={project.projectItems ?? []} palette={project.labelPalette ?? []} bind:selected={projectSelection} clipSelection={timelineSelection} onselection={adoptLabelSelection} clipLabel={sequence?.clips?.find(clip => clip.id === clipId)?.label} disabled={busy} onedit={commitEdit} />
      <FilmcraftClipEditControls document={project.documentText} {sequenceId} {clipId} {token} selection={timelineSelection} {linked} tracks={sequence?.tracks ?? []} timeFrames={editPositionFrames} timeTicks={pastePositionTicks || undefined} disabled={busy} onedit={commitEdit} />
      <FilmcraftMediaModifyControls document={project.documentText} {sequenceId} {clipId} {token} selection={timelineSelection} selected={projectSelection} items={project.projectItems ?? []} sourceBindings={activeBindings} disabled={busy} onedit={commitEdit} />
      <FilmcraftVideoOptionsControls document={project.documentText} {sequenceId} {clipId} {token} selection={timelineSelection} timeFrames={editPositionFrames} timeTicks={pastePositionTicks || undefined} disabled={busy} onedit={commitEdit} />
      <FilmcraftReplaceControls document={project.documentText} {sequenceId} {clipId} {token} selection={timelineSelection} {projectSelection} {sourceItem} {sourcePlayhead} {linked} items={project.projectItems ?? []} timeFrames={editPositionFrames} timeTicks={pastePositionTicks || undefined} disabled={busy} onedit={commitEdit} />
      <FilmcraftAttributesControls document={project.documentText} {sequenceId} {clipId} {token} selection={timelineSelection} {linked} clipboardText={clipboard?.clipboardText} disabled={busy} onedit={commitEdit} />
      <FilmcraftSubclipControls document={project.documentText} {sequenceId} {clipId} {token} items={project.projectItems ?? []} {sourceItem} {sourcePlayhead} selection={timelineSelection} selected={projectSelection} bind:stillDuration={stillImageDuration} bind:stillUnit={stillImageUnit} disabled={busy} onedit={commitEdit} />
      <FilmcraftMarksControls bind:stillDuration={stillImageDuration} bind:stillUnit={stillImageUnit} tracks={sequence?.tracks ?? []} bind:rippleMarkers bind:item={sourceItem} {sourcePlayhead} selection={timelineSelection} {projectSelection} linkedSelection={linked} oncontext={adoptNavigation} marks={sequence?.marks} document={project.documentText} {sequenceId} {clipId} {token} items={project.projectItems ?? []} timeFrames={editPositionFrames} timeTicks={pastePositionTicks || undefined} disabled={busy} onedit={commitEdit} />
      <FilmcraftBinActions revealRequest={projectReveal} document={project.documentText} {sequenceId} {clipId} {token} items={project.projectItems ?? []} bins={project.projectBins ?? []} clipSelection={timelineSelection} bind:selected={projectSelection} disabled={busy} onedit={commitEdit} />
      {#if renameBins.length}<label>Папка проекта <select aria-label="Папка проекта Filmcraft" value={renameBinId} onchange={(event) => { renameBinId = event.currentTarget.value }} disabled={busy}><option value="">Выбрать папку для переименования</option>{#each renameBins as bin}<option value={bin.id}>{'—'.repeat(Math.min(bin.depth, 10))} {bin.name || bin.id}</option>{/each}</select></label>{/if}
      {#if renameBinId && renameBins.some(bin => bin.id === renameBinId)}<FilmcraftRenameControls document={project.documentText} {sequenceId} {clipId} {token} binId={renameBinId} name={renameBins.find(bin => bin.id === renameBinId)?.name || ''} disabled={busy} onedit={commitEdit} />{/if}
      <label>Элемент проекта <select aria-label="Элемент проекта Filmcraft" value={renameItemId} onchange={(event) => { renameItemId = event.currentTarget.value }} disabled={busy}><option value="">Выбрать элемент для переименования</option>{#each renameItems as item}<option value={item.id}>{item.kind}: {item.name || item.id}</option>{/each}</select></label>
      {#if renameItemId && renameItems.some(item => item.id === renameItemId)}<FilmcraftRenameControls document={project.documentText} {sequenceId} {clipId} {token} itemId={renameItemId} name={renameItems.find(item => item.id === renameItemId)?.name || ''} disabled={busy} onedit={commitEdit} />{/if}
      {#if clipId}<FilmcraftRenameControls document={project.documentText} {sequenceId} {clipId} {token} name={sequence?.clips?.find(clip => clip.id === clipId)?.name || ''} disabled={busy} onedit={commitEdit} />{/if}
      {#if clipId}<FilmcraftScalarParameterEditor trackKind={sequence?.clips?.find(clip => clip.id === clipId)?.trackKind === 'audio_tracks' ? 'Audio' : 'Video'} sources={activeBindings} document={project.documentText} {sequenceId} {clipId} {token} timeFrames={editPositionFrames} timeTicks={pastePositionTicks || undefined} disabled={busy} onedit={commitEdit} />{/if}
      {#key sequenceId}<FilmcraftVideoExportPanel document={project.documentText} {sequenceId} sources={activeBindings} imageSources={library.filter(entry => entry.kind === 'source' && (entry.mediaType === 'image' || /\.(png|jpe?g|webp|gif|bmp|tiff?)$/i.test(entry.filename)))} {token} initialFrames={sequence?.durationFrames ?? '24'} disabled={busy} />{/key}
      <FilmcraftProgramPreview sources={activeBindings} document={project.documentText} {sequenceId} timeFrames={editPositionFrames} {token} />
      {#if sequence?.clips?.length}
        <label>Клип <select aria-label="Клип Filmcraft" value={clipId} onchange={(event) => { clipId = event.currentTarget.value; rightClipId = ''; extraSelectedIds = [] }} disabled={busy}><option value="">Выберите клип</option>{#each sequence.clips as clip}<option value={clip.id}>{clip.name || clip.id} · {clip.trackKind === 'video_tracks' ? 'Видео' : 'Звук'} {clip.trackId}</option>{/each}</select></label>
        <label>Режим <select aria-label="Режим обрезки" value={trimMode} onchange={(event) => trimMode = event.currentTarget.value as 'regular' | 'ripple'} disabled={busy}><option value="regular">Обычная</option><option value="ripple">Со сдвигом</option></select></label>
        <label>Край <select bind:value={edge} disabled={busy}><option value="in">Начало</option><option value="out">Конец</option></select></label>
        <label>Сдвиг, кадров <input aria-label="Сдвиг обрезки" type="number" step="1" bind:value={trimFrames} disabled={busy} /></label>
        <label class="linked"><input aria-label="Связанные клипы" type="checkbox" bind:checked={linked} disabled={busy} />Связанные клипы</label>
        <button type="button" disabled={busy || !clipId || !Number.isSafeInteger(trimFrames)} onclick={() => editClip()}>Обрезать</button>
        <button type="button" disabled={busy || !clipId || !Number.isSafeInteger(trimFrames)} onclick={() => editClip('slip')}>Сдвинуть исходный участок</button>
        <button type="button" disabled={busy || !clipId || !Number.isSafeInteger(trimFrames)} onclick={() => editClip('stretch')}>Растянуть со сменой скорости</button>
        <button type="button" disabled={busy || !clipId || !Number.isSafeInteger(trimFrames)} onclick={() => editClip('slide')}>Переместить с подрезкой соседей</button>
        <label>Выбрать также <select aria-label="Дополнительные клипы" multiple size="3" value={extraSelectedIds} onchange={(event) => extraSelectedIds = [...event.currentTarget.options].filter(option => option.selected).map((option) => option.value)} disabled={busy}>{#each sequence.clips.filter((clip) => clip.id !== clipId) as clip}<option value={clip.id}>{clip.name || clip.id} · {clip.trackKind === 'video_tracks' ? 'Видео' : 'Звук'} {clip.trackId}{clip.locked ? ' · Заблокирован' : ''}</option>{/each}</select></label>
        <span>Выбрано клипов: {selectedIds.length}</span>
        <button type="button" disabled={busy || !clipId || !validEditPosition} onclick={() => editClip('close-gap')}>Закрыть зазор на дорожке</button>
        <button type="button" disabled={busy || !clipId || !validEditPosition} onclick={() => editClip('razor')}>Разрезать выбранные клипы</button>
        <button type="button" disabled={busy || !validEditPosition} onclick={() => editClip('razor-all')}>Разрезать все дорожки</button>
        <label>Длительность диапазона, кадров <input aria-label="Длительность диапазона, кадров" type="number" min="0" step="1" bind:value={rangeDurationFrames} disabled={busy} /></label>
        <button type="button" disabled={busy || !clipId || !validEditPosition || !Number.isSafeInteger(rangeDurationFrames) || rangeDurationFrames < 0} onclick={() => editClip('lift-range')}>Удалить диапазон с зазором</button>
        <button type="button" disabled={busy || !clipId || !validEditPosition || !Number.isSafeInteger(rangeDurationFrames) || rangeDurationFrames < 0} onclick={() => editClip('extract-range')}>Удалить диапазон со сдвигом</button>
        <button type="button" disabled={busy || !clipId} onclick={() => editClip('delete')}>Удалить клип с зазором</button>
        <button type="button" disabled={busy || !clipId} onclick={() => editClip('ripple-delete')}>Удалить клип со сдвигом</button>
        <label>Правый клип <select aria-label="Правый клип стыка" value={rightClipId} onchange={(event) => rightClipId = event.currentTarget.value} disabled={busy}><option value="">Выберите соседний клип</option>{#each rightClips as clip}<option value={clip.id}>{clip.name || clip.id}</option>{/each}</select></label>
        <button type="button" disabled={busy || !rightClips.some((clip) => clip.id === rightClipId) || !Number.isSafeInteger(trimFrames)} onclick={() => editClip('roll')}>Сдвинуть стык</button>
        <details class="operation-help"><summary>Как работают операции</summary><small>Сдвиг исходного участка меняет содержимое выбранного клипа, сохраняя его положение и длительность. Растяжение двигает выбранный край и меняет скорость, сохраняя исходный участок. Сдвиг стыка удлиняет один клип и укорачивает соседний, сохраняя общую длину. Перемещение с подрезкой сохраняет длительность выбранного клипа и меняет соседей у его краёв. Удаление и разрезание выбранных клипов учитывают дополнительные клипы и флажок связей. Разрезание всех дорожек пропускает заблокированные дорожки. Позиция правки задаёт начало диапазона, разреза или зазора от начала секвенции. Диапазон удаляется на дорожках выбранных клипов; со сдвигом также обрабатываются синхронизированные дорожки и субтитры. Остальные операции используют поле сдвига.</small></details>
      {/if}
      <details class="transition-controls" ontoggle={(event) => { if (event.currentTarget.open) void loadTransitions() }}>
        <summary>Переходы</summary>
        <div class="controls">
          {#if transitionsLoading}<span>Загрузка переходов…</span>{/if}
          {#if transitionsError}<span role="alert">{transitionsError}</span><button type="button" onclick={loadTransitions}>Повторить загрузку</button>{/if}
          <label>Правка <select aria-label="Созданный переход Filmcraft" value={editingTransitionId} onchange={(event) => chooseAppliedTransition(event.currentTarget.value)} disabled={busy}>
            <option value="">Новый переход</option>{#each sequence?.transitions || [] as item}<option value={item.id}>{transitions.find((entry) => entry.definition.id === item.effect)?.definition.name || item.effect} · {item.trackKind === 'audio_tracks' ? 'Звук' : 'Видео'} {item.trackId} · {pasteTime(item.startTicks)} с</option>{/each}
          </select></label>
          {#if editingTransition && !transition}<span>Эффект отсутствует в каталоге: {editingTransition.effect}</span>{/if}
          {#if transition}
            {#if !editingTransition}<label>Место <select aria-label="Место перехода" value={transitionTarget} onchange={(event) => { transitionTarget = event.currentTarget.value as 'edge' | 'playhead' }} disabled={busy}><option value="edge">Край клипа</option><option value="playhead">Позиция правки</option></select></label>
            <label>Эффект <select aria-label="Переход Filmcraft" value={transition.id} disabled={busy} onchange={(event) => { transitionId = event.currentTarget.value }}>
              {#each availableTransitions as entry}<option value={entry.definition.id}>{entry.definition.category.at(-1)} · {entry.definition.name}</option>{/each}
            </select></label>
            <label>Длительность <input aria-label="Длительность перехода" inputmode="decimal" bind:value={transitionFrames} disabled={busy} /></label><label>Единицы <select aria-label="Единицы длительности перехода" value={transitionUnit} onchange={(event) => { transitionUnit = event.currentTarget.value as 'frames' | 'seconds' }} disabled={busy}><option value="frames">Кадры</option><option value="seconds">Секунды</option></select></label>
            {/if}<label class="linked"><input aria-label="Обратный переход" type="checkbox" bind:checked={transitionReverse} disabled={busy} />Обратно</label>
            {#each transition.params as param (transition.id + param.id)}
              {#if 'Bool' in param.default}<label class="linked"><input aria-label={`Параметр перехода: ${param.label}`} type="checkbox" checked={parameterValue(param) === 'true'} onchange={(event) => setParameter(param, String(event.currentTarget.checked))} disabled={busy} />{param.label}</label>
              {:else if parameterChoices(param).length}<label>{param.label}<select aria-label={`Параметр перехода: ${param.label}`} value={parameterValue(param)} onchange={(event) => setParameter(param, event.currentTarget.value)} disabled={busy}>{#each parameterChoices(param) as option, index}<option value={String(index)}>{option}</option>{/each}</select></label>
              {:else}<label>{param.label}{'Vec2' in param.default ? ' · X, Y' : ''}<input aria-label={`Параметр перехода: ${param.label}`} value={parameterValue(param)} placeholder={'Vec2' in param.default ? 'Авто' : ''} oninput={(event) => setParameter(param, event.currentTarget.value)} disabled={busy} /></label>{/if}
            {/each}
            {#if editingTransition}
              <button type="button" disabled={busy} onclick={() => updateTransition()}>Сохранить переход</button>
              <button type="button" disabled={busy} onclick={() => updateTransition(true)}>Сбросить параметры перехода</button>
            {:else}
            <button type="button" disabled={busy || !validTransitionTarget || !validTransitionFrames || !validEditPosition} onclick={applyTransition}>Применить переход</button>
            <small>{#if transitionTarget === 'playhead'}Поиск стыка у позиции правки на дорожках выбранных клипов ({transitionTargetTracks.length}). Учитывается тип основного клипа. Стык должен быть ближе трёх кадров.{:else}Переход применяется к выбранному краю клипа. Если соседний клип касается стыка, переход соединяет их.{/if}</small>
            {/if}
          {/if}
        </div>
        {#if transitions.length}
          <details class="batch-transitions"><summary>Переходы выделенным клипам</summary><div class="controls">
            <label>Видео <select aria-label="Пакетный видеопереход" value={batchVideoEffect} onchange={(event) => { batchVideoEffect = event.currentTarget.value }} disabled={busy}>{#if !transitions.some((entry) => entry.definition.kind === 'VideoTransition' && entry.definition.id === batchVideoEffect)}<option value={batchVideoEffect}>Недоступен: {batchVideoEffect}</option>{/if}{#each transitions.filter((entry) => entry.definition.kind === 'VideoTransition') as entry}<option value={entry.definition.id}>{entry.definition.name}</option>{/each}</select></label>
            <label>Длительность видео <input aria-label="Длительность пакетного видеоперехода" inputmode="decimal" bind:value={batchVideoValue} disabled={busy} /></label><label>Единицы <select aria-label="Единицы пакетного видеоперехода" value={batchVideoUnit} onchange={(event) => { batchVideoUnit = event.currentTarget.value as 'frames' | 'seconds' }} disabled={busy}><option value="frames">Кадры</option><option value="seconds">Секунды</option></select></label>
            <label>Звук <select aria-label="Пакетный аудиопереход" value={batchAudioEffect} onchange={(event) => { batchAudioEffect = event.currentTarget.value }} disabled={busy}>{#if !transitions.some((entry) => entry.definition.kind === 'AudioTransition' && entry.definition.id === batchAudioEffect)}<option value={batchAudioEffect}>Недоступен: {batchAudioEffect}</option>{/if}{#each transitions.filter((entry) => entry.definition.kind === 'AudioTransition') as entry}<option value={entry.definition.id}>{entry.definition.name}</option>{/each}</select></label>
            <label>Длительность звука <input aria-label="Длительность пакетного аудиоперехода" inputmode="decimal" bind:value={batchAudioValue} disabled={busy} /></label><label>Единицы <select aria-label="Единицы пакетного аудиоперехода" value={batchAudioUnit} onchange={(event) => { batchAudioUnit = event.currentTarget.value as 'frames' | 'seconds' }} disabled={busy}><option value="frames">Кадры</option><option value="seconds">Секунды</option></select></label>
            <button type="button" disabled={busy || !selectedIds.length || !validBatchTransitions} onclick={() => applySelectionTransitions()}>Применить к выделенным</button>
            <label>Ближайшие точки <select aria-label="Режим ближайших точек" value={nearestKind} onchange={(event) => { nearestKind = event.currentTarget.value as FilmcraftNearestEditKind }} disabled={busy}><option value="roll">Стык соседних клипов</option><option value="trimIn">Начало · обычная подрезка</option><option value="trimOut">Конец · обычная подрезка</option><option value="rippleIn">Начало · со сдвигом</option><option value="rippleOut">Конец · со сдвигом</option></select></label>
            <label>Дорожки <select aria-label="Дорожки ближайших точек" multiple value={nearestTargets} onchange={(event) => { nearestTargets = [...event.currentTarget.options].filter(option => option.selected).map((option) => option.value) }} disabled={busy}>{#each nearestTracks as track}<option value={track.id}>{track.kind === 'video_tracks' ? 'Видео' : 'Звук'} {track.id}</option>{/each}</select></label>
            <button type="button" disabled={busy || !validEditPosition} onclick={selectNearestPoints}>Выбрать ближайшие точки</button>
            <button type="button" disabled={busy || (!pastePositionTicks && !validEditPosition) || (!selectedIds.length && !nearestTargets.length)} onclick={() => trimRangeToPlayhead('previous', false)}>Удалить до курсора с зазором</button>
            <button type="button" disabled={busy || (!pastePositionTicks && !validEditPosition) || (!selectedIds.length && !nearestTargets.length)} onclick={() => trimRangeToPlayhead('next', false)}>Удалить после курсора с зазором</button>
            <button type="button" disabled={busy || (!pastePositionTicks && !validEditPosition) || (!selectedIds.length && !nearestTargets.length)} onclick={() => trimRangeToPlayhead('previous', true)}>Удалить до курсора со сдвигом</button>
            <button type="button" disabled={busy || (!pastePositionTicks && !validEditPosition) || (!selectedIds.length && !nearestTargets.length)} onclick={() => trimRangeToPlayhead('next', true)}>Удалить после курсора со сдвигом</button>
            <small>По одной точке на дорожку у позиции правки. Без выбранных дорожек — на всех. Выделение клипов снимается.</small>
            <label>Сторона точки <select aria-label="Сторона точки монтажа" value={editPointEdge} onchange={(event) => { editPointEdge = event.currentTarget.value as 'in' | 'out' }} disabled={busy}><option value="in">Начало клипа</option><option value="out">Конец клипа</option></select></label>
            <button type="button" disabled={busy || !clipId || editPoints.length >= 4096} onclick={() => { resetTrimShift(); editPoints = [...editPoints, { clipId, edge: editPointEdge }] }}>Добавить точку монтажа</button>
            {#if editPoints.length}
              <ol aria-label="Выбранные точки монтажа">
                {#each editPoints as point, index}
                  <li>{sequence?.clips?.find((clip) => clip.id === point.clipId)?.name || 'Клип'} · {point.edge === 'in' ? 'начало' : 'конец'} · {point.kind === 'roll' ? 'стык' : point.kind === 'ripple' ? 'со сдвигом' : 'подрезка'} <button type="button" aria-label={`Удалить точку монтажа ${index + 1}`} disabled={busy} onclick={() => { resetTrimShift(); editPoints = editPoints.filter((_, i) => i !== index) }}>×</button></li>
                {/each}
              </ol>
              <label>Сдвиг точек, кадров <input aria-label="Сдвиг точек монтажа, кадров" inputmode="numeric" bind:value={pointTrimFrames} disabled={busy} /></label>
              <button type="button" disabled={busy || !validPointTrim} onclick={() => trimPoints()}>Подрезать выбранные точки</button>
              <button type="button" disabled={busy || !validEditPosition} onclick={() => trimPoints(true)}>Подрезать точки до курсора</button>
              <button type="button" disabled={busy} onclick={() => { editPoints = editPoints.map((point) => ({ ...point, kind: point.kind === 'ripple' ? 'roll' : point.kind === 'roll' ? 'trim' : 'ripple' })); pointTrimCount = null }}>Сменить тип подрезки точек</button>
              {#if pointTrimCount !== null}<span role="status">Обработано точек: {pointTrimCount}</span>{/if}
              <small>Подрезка идёт по порядку. При поздней ошибке предыдущие правки сохраняются; их можно отменить.</small>
              <button type="button" disabled={busy || !validBatchTransitions} onclick={() => applySelectionTransitions(true)}>Применить к точкам монтажа</button>
              <button type="button" disabled={busy} onclick={() => { resetTrimShift(); editPoints = [] }}>Очистить точки монтажа</button>
              <details class="trim-monitor" open={monitorOpen} ontoggle={(event) => { monitorOpen = event.currentTarget.open }}><summary>Монитор стыка</summary>
                <div aria-label="Счётчики сдвига точек">Исходящая сторона: {trimShiftFrames.outShift} · Входящая сторона: {trimShiftFrames.inShift}</div>
                <small>Сдвиг в кадрах секвенции; новая выборка точек начинает отсчёт заново.</small>
                <small>Первая точка списка. Кадры доступны для генераторов и файлов из библиотеки.</small>
                {#if monitorLoading}<span role="status">Загрузка данных стыка…</span>
                {:else if monitorError}<p role="alert">{monitorError}</p><button type="button" onclick={() => { monitorRetry += 1 }}>Повторить загрузку стыка</button>
                {:else if monitor}
                  {#if monitor.editTime === null}<p>Точка монтажа недоступна.</p>{/if}
                  <div class="controls">
                    {#each [{ label: 'Исходящий кадр', side: monitor.outgoing }, { label: 'Входящий кадр', side: monitor.incoming }] as view}
                      <div><strong>{view.label}</strong>{#if view.side}{#if view.side.frame}<FilmcraftMonitorFrame frame={view.side.frame} label={view.label} />{:else if view.side.video}<small>Просмотр этого исходника пока недоступен.</small>{/if}<div>{view.side.name} · {view.side.track}</div><div>Кадр исходника: {view.side.mediaFrame}</div><div>Позиция исходника: {view.side.mediaTime}</div><div>Исходный участок: {view.side.sourceIn} — {view.side.sourceOut}</div>{:else}<div>Клипа с этой стороны нет</div>{/if}</div>
                    {/each}
                  </div>
                  {#if monitor.outgoing?.frameUnavailable || monitor.incoming?.frameUnavailable}
                    <button type="button" onclick={() => { monitorRetry += 1 }}>Повторить загрузку кадров</button>
                  {/if}
                  <small>Позиции и границы указаны в точных тиках проекта.</small>
                {/if}
              </details>
              <small>Точки обрабатываются по порядку, включая повторы. Переход на выбранном краю заменяется; недоступные точки пропускаются.</small>
            {/if}
            {#if batchTransitionCount !== null}<span role="status">Создано переходов: {batchTransitionCount}</span>{/if}
            <small>Оба края выделенных клипов; учитывается флажок «Связанные клипы». Существующие переходы на стыках сохраняются. Заблокированные края пропускаются; успешные изменения отменяются одним действием.</small>
          </div></details>
        {/if}
      </details>
      <details class="dynamic-trim-controls"><summary>Динамическая подрезка и просмотр</summary>
        <FilmcraftTrimPlaybackControls {token} {project} disabled={busy || (!pastePositionTicks && !validEditPosition)} input={{ document: project.documentText, sequenceId, points: editPoints.map(point => ({ clip: point.clipId, out: point.edge === 'out', kind: point.kind || 'trim' })), linked, shift: trimShift, ...(pastePositionTicks ? { playheadTicks: pastePositionTicks } : { playheadFrames: editPositionFrames }) }}
          onActive={(active) => { busy = active }} onError={(message) => { error = message }}
          onPreview={(edited) => { clearDownload(); project = edited; reconcileSelection(edited); trimShift = edited.shift; trimShiftFrames = edited.shiftFrames; editPositionFrames = edited.status.playheadFrames; pastePositionTicks = edited.status.playheadTicks; pastePlayheadTicks = ''; pointTrimCount = null; batchTransitionCount = null }}
          onCommit={(before, edited) => { if (before.documentText !== edited.documentText) { undo = boundedHistory([...undo, before]); redo = [] } }} />
      </details>
      <label class="linked"><input aria-label="Копировать маркеры секвенции" type="checkbox" bind:checked={includeMarkers} disabled={busy} />Копировать маркеры секвенции</label>
      <label class="linked"><input aria-label="Сдвигать маркеры при вставке" type="checkbox" bind:checked={rippleMarkers} disabled={busy} />Сдвигать маркеры при вставке</label>
      <button type="button" disabled={busy || !selectedIds.length} onclick={copySelection}>Копировать выбранные клипы</button>
      <button type="button" disabled={busy || !clipboard || !sequenceId || (!pastePositionTicks && !validEditPosition)} onclick={() => pasteSelection(false)}>Вставить с перезаписью</button>
      <button type="button" disabled={busy || !clipboard || !sequenceId || (!pastePositionTicks && !validEditPosition)} onclick={() => pasteSelection(true)}>Вставить со сдвигом</button>
      {#if clipboard}<span>В буфере: {clipboard.clipCount} клипов, {clipboard.markerCount} маркеров</span>{/if}
      {#if pastePlayheadTicks}<span>Конец вставки: {pasteTime(pastePlayheadTicks)} с</span>{/if}
      <button type="button" disabled={busy || !undo.length} onclick={() => restore('undo')}>Отменить правку</button>
      <button type="button" disabled={busy || !redo.length} onclick={() => restore('redo')}>Повторить правку</button>
      {#if download}<a href={download} download={`sequence.${format}`}>Скачать {format.toUpperCase()}</a>{/if}
      <small>Звук: WAV и встроенные генераторы. Для длинной секвенции выберите участок до {Math.floor(4194304 / (sequence?.sampleRate || 48000))} с.</small>
    {/if}
  </div>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>
  .filmcraft-panel { font-size: 0.8rem; padding-block: 0.5rem; }
  summary { cursor: pointer; }
  .controls { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: end; padding-block: 0.5rem; }
  label { display: flex; flex-direction: column; gap: 0.2rem; }
  label.linked { flex-direction: row; align-items: center; align-self: end; }
  input[type='number'], input[inputmode='numeric'] { width: 6rem; }
  select { max-width: 100%; }
  .transition-controls { flex-basis: 100%; min-width: 0; }
  .trim-monitor { flex-basis: 100%; min-width: 0; }
  .trim-monitor .controls { align-items: start; }
  .transition-controls label { max-width: 100%; }
  .transition-controls input:not([type=checkbox]) { width: 9rem; }
  small, .operation-help { flex-basis: 100%; }
</style>
