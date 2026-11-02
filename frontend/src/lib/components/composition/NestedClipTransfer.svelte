<script lang="ts">
  import NestedSourceSpeedCurve from './NestedSourceSpeedCurve.svelte'
  import {concatSelectionKeyMarks,concatKeyTargetFromMarks,concatKeyMarkerGroups} from '$lib/composition/concatKeyNavigation'
  import NestedSourceKeyMarks from './NestedSourceKeyMarks.svelte'
  import {nativeGapAt} from '$lib/composition/nativeRipple'
  import {agentSession} from '$lib/agent/session.svelte'
  import {slipNestedLayer,raiseNestedLayers,rippleNestedComposition} from '$lib/state/nestedComposition.svelte'
  import {activityReveal} from '$lib/agent/activityReveal.svelte'
  import NestedCaptionTextEditor from './NestedCaptionTextEditor.svelte'
  import { untrack,tick } from 'svelte'
  import {SvelteSet} from 'svelte/reactivity'
  import VideoEnhancementPanel from './VideoEnhancementPanel.svelte'
  import PersonVideoPanel from './PersonVideoPanel.svelte'
  import {attachNestedVideoRegion,replaceNestedCutoutMedia,replaceNestedVideoCutoutMedia} from '$lib/state/nestedComposition.svelte'
  import ImageCutoutPanel from './ImageCutoutPanel.svelte'
  import NestedConcatCutout from './NestedConcatCutout.svelte'
  import NestedConcatVideoCutout from './NestedConcatVideoCutout.svelte'
  import ImageEnhancementPanel from './ImageEnhancementPanel.svelte'
  import {authState} from '$lib/state/auth.svelte'
  import { listenMany, eventListener } from '../../../composables/globalListeners'
  import { shortcutCommandForEvent, shortcutState, shortcutLabel } from '$lib/state/shortcuts.svelte'
  import { nestedComposition, nestedHistory, freezeNestedVideoClip, stepNestedConcatKey, clearNestedConcatKeys, quickPoseNestedClips, mergeNestedCaptionClips, moveNestedClips, removeNestedClips, trimNestedClipEnds, trimNestedClipStarts, editNestedClipLinks, splitNestedClips, replaceNestedMedia, switchNestedMediaVersion, duplicateNestedClips, copyNestedClips, nestedClipboard, copyNestedSelectionToClipboard, pasteNestedClipboard, cutNestedSelectionToClipboard, undoNestedComposition, redoNestedComposition, importNestedClipboardText } from '$lib/state/nestedComposition.svelte'
  import type { WireCompositionTrack } from '$lib/composition/types'
  import { nativeClipEnd } from '$lib/composition/nativeClipInsert'
  import { pasteNativeCaptionTracks } from '$lib/composition/nativeCaptionClipboard'
  import { serializeNativeClipboard } from '$lib/composition/nativeClipboardText'
  import { captionDisplaySeconds } from '$lib/composition/captionTime'
  import { prepareNativeClipboardPaste } from '$lib/composition/nativeClipboard'
  import { prepareNativeClipCopy } from '$lib/composition/nativeClipCopy'
  import { duplicateNativeClips } from '$lib/composition/nativeClipDuplicate'
  import { preflightNativeClipTrim } from '$lib/composition/concatSourceTrim'
  import { uploadFile } from '$lib/api'
  import { loadLibrary } from '$lib/state/store.svelte'
  import { state as library } from '$lib/state/store.svelte'
  import { compositionSourceFromLibraryEntry } from '$lib/state/composition.svelte'
  import { replaceNativeMedia, setNativeMediaVersion } from '$lib/composition/nativeMediaReplace'
  import { expandNativeClipLinks, linkNativeClips, unlinkNativeClips } from '$lib/composition/nativeClipLinks'
  import { nativeClipMoveBatch, defaultNativeTargetTrack, nativeClipStart } from '$lib/composition/nativeClipMove'
  import { orderedTimelineIds } from '$lib/composition/timelineOrder'
  let { disabled = false }: { disabled?: boolean } = $props()
  const project = $derived(nestedComposition.document)
  const active = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  const tracks = $derived(project?.compositions[active]?.content.tracks ?? [])
  let rippleTrack=$state('')
  const rippleLane=$derived(tracks.find(track=>track.id===rippleTrack)??tracks[0])
  const rippleGap=$derived(rippleLane?nativeGapAt(rippleLane,nestedComposition.timeUs):null)
  const rippleBlocked=$derived(disabled||nestedHistory.busy||Boolean(agentSession.lock))
  let selected = $state<string[]>([]), destination = $state(''), seconds = $state(0), trimDelta = $state(0), leftDelta = $state(0), targets = $state<Record<string, string>>({})
  const sourceKeyMarks=$derived(project?concatSelectionKeyMarks(project,active,selected):[])
  const sourceKeyGroups=$derived(concatKeyMarkerGroups(sourceKeyMarks))
  const sourceKeyRange=$derived(sourceKeyMarks.reduce((range,key)=>[Math.min(range[0]!,key.startUs),Math.max(range[1]!,key.endUs)],[Infinity,-Infinity]))
  const raiseReason=$derived.by(()=>{
    if(!selected.length)return 'Выберите клипы'
    const rows=tracks.filter(track=>track.clips.some(clip=>selected.includes(clip.id)))
    if(rows.some(track=>track.locked))return 'Дорожка заблокирована'
    if(new Set(rows.map(track=>track.kind)).size!==1)return 'Выберите клипы одного типа'
    return null
  })
  function raiseSelection(atCursor:boolean){
    if(rippleBlocked||raiseReason)return
    const scope=active,ids=[...selected],anchor=atCursor?{clipId:ids[0]!,startTicks:nestedComposition.timeUs}:undefined
    void keepKeyboardFocus(()=>raiseNestedLayers(scope,ids,`track-${crypto.randomUUID()}`,anchor))
  }
  let slipSeconds=$state(0),slipClipId=$state('')
  const slipRows=$derived(tracks.flatMap(track=>track.clips.filter(clip=>selected.includes(clip.id)&&'placement' in clip).map(clip=>({track,clip}))))
  const slipLocation=$derived(slipRows.find(row=>row.clip.id===slipClipId)??slipRows[0])
  const slipReason=$derived.by(()=>{
   const clip=slipLocation?.clip
   if(!clip||!('placement' in clip))return 'Выберите видео- или аудиоклип'
   if(slipLocation!.track.locked)return 'Дорожка заблокирована'
   if(clip.placement.speedRamp||'playbackMode' in clip&&clip.playbackMode.mode==='freeze')return 'Стоп-кадр и переменную скорость нельзя сдвигать'
   return null
  })
  let copyStartSeconds = $state(0)
  let captionSelected = $state<string[]>([])
  const captionTracks = $derived(project?.compositions[active]?.content.captionTracks ?? [])
  const clipboardCaptionTracks = $derived(nestedClipboard.value?.compositions[nestedClipboard.value.root]?.content.captionTracks ?? [])
  let clipboardTargets = $state<Record<string,string>>({})
  const clipboardTracks = $derived(nestedClipboard.value?.compositions[nestedClipboard.value.root]?.content.tracks.filter(track=>track.clips.length) ?? [])
  const clipboardCount = $derived(clipboardTracks.reduce((count,track)=>count+track.clips.length,0)+clipboardCaptionTracks.reduce((count,track)=>count+track.clips.length,0))
  $effect(()=>{ void nestedClipboard.value; clipboardTargets={} })
  const sourceLabels = $derived(new Map(library.library.map(entry => [entry.id, entry.title?.trim() || entry.filename])))
  function clipTitle(clip: WireCompositionTrack['clips'][number], kind: WireCompositionTrack['kind']): string {
    const metadata = project?.compositions[active]?.content.metadata
    const name = metadata?.clipNames?.[clip.id]
    if (name?.trim()) return name
    if ('sourceId' in clip) { const source = metadata?.sourceNames?.[clip.sourceId] || sourceLabels.get(clip.sourceId); if (source) return source }
    if ('text' in clip && clip.text.trim()) return clip.text.trim().split('\n')[0]!.slice(0,40)
    return { video: 'Видео', audio: 'Звук', image: 'Изображение', text: 'Текст', color: 'Цвет', graphic: 'Графика' }[kind]
  }
  const freezeTarget=$derived.by(()=>selected.length===1?tracks.flatMap(track=>track.kind==='video'&&!track.locked?track.clips:[]).find(clip=>clip.id===selected[0]):undefined)
  async function freezeAtCursor(){
    const target=freezeTarget,scope=active,selection=JSON.stringify(selected)
    if(disabled||nestedHistory.busy||!target||!authState.token)return
    const still=await freezeNestedVideoClip(authState.token,scope,target.id)
    if(still&&active===scope&&JSON.stringify(selected)===selection){selected=[still];void loadLibrary()}
  }
  const enhancementTarget=$derived.by(()=>{if(selected.length!==1)return undefined;for(const track of tracks){if(track.kind!=='image')continue;const clip=track.clips.find(clip=>clip.id===selected[0]);if(!clip)continue;const source=project?.compositions[active]?.content.sources[clip.sourceId];if(source?.kind==='image')return {clipId:clip.id,source}}return undefined})
  const videoEnhancementTarget=$derived.by(()=>{
    if(!project||!selected.length)return undefined
    const selectedClips=tracks.flatMap(track=>track.clips.filter(clip=>selected.includes(clip.id)).map(clip=>({clip,kind:track.kind})))
    const seed=selectedClips.find(value=>value.kind==='video'&&'sourceId' in value.clip)
    if(!seed||!('sourceId' in seed.clip))return undefined
    const source=project.compositions[active]?.content.sources[seed.clip.sourceId]
    if(source?.kind!=='video'||selectedClips.some(value=>(value.kind!=='video'&&value.kind!=='audio')||!('sourceId' in value.clip)||value.clip.sourceId!==source.id))return undefined
    const members=expandNativeClipLinks(project,active,[seed.clip.id]);if(members.length!==selected.length||members.some(id=>!selected.includes(id)))return undefined
    return {clipId:seed.clip.id,source,members}
  })
  let replacementSourceId = $state(''), keepOriginal = $state(false), changeSourceStart = $state(false), replacementStart = $state(0), uploadBusy = $state(false)
  let keyboardScope = $state<HTMLFieldSetElement>()
  let systemClipboardBusy=$state(false), systemClipboardStatus=$state(''), systemClipboardError=$state('')
  let alive = true
  $effect(() => () => { alive = false })
  let previousActive = ''
  $effect(() => { if (active && active !== previousActive) { previousActive = active; selected = []; captionSelected = []; replacementSourceId = ''; targets = {}; destination = untrack(() => Object.keys(project?.compositions ?? {})).find(id => id !== active) ?? active } })
  $effect(() => {
    if (!project || !project.compositions[active]) return
    const known = new Set(tracks.flatMap(track => track.clips.map(clip => clip.id)))
    const previous = untrack(() => selected)
    const next = expandNativeClipLinks(project, active, previous.filter(id => known.has(id)))
    if (next.length !== previous.length || next.some((id, index) => id !== previous[index])) selected = next
  })
  $effect(()=>{const known=new Set(captionTracks.flatMap(track=>track.clips.map(cue=>cue.id)));const previous=untrack(()=>captionSelected), next=previous.filter(id=>known.has(id));if (next.length!==previous.length) captionSelected=next})
  let revealedRequest=''
  $effect(()=>{
    const request=activityReveal.request,scope=keyboardScope
    if(!request||request.mode!=='nested'||!request.clipId||request.compositionId!==active||request.id===revealedRequest||!scope)return
    revealedRequest=request.id
    if(!tracks.some(track=>track.clips.some(clip=>clip.id===request.clipId)))return
    selected=expandNativeClipLinks(project!,active,[request.clipId])
    void tick().then(()=>{
      if(!alive||activityReveal.request!==request||!scope.isConnected)return
      const input=[...scope.querySelectorAll<HTMLInputElement>('[data-clip-id]')].find(input=>input.dataset.clipId===request.clipId)
      input?.scrollIntoView({block:'nearest',inline:'nearest'});(input&&!input.disabled?input:scope).focus({preventScroll:true})
    })
  })
  function select(id: string, checked: boolean): void {
    if (!project) return
    try {
      if (checked) selected = expandNativeClipLinks(project, active, [...selected, id])
      else { const group = new SvelteSet(expandNativeClipLinks(project, active, [id])); selected = selected.filter(id => !group.has(id)) }
    } catch (error) { nestedHistory.error = error instanceof Error ? error.message : String(error) }
  }
  function reason(): string | null {
    if (!project) return 'Нет открытого проекта'
    let n = 0
    const reserved = new SvelteSet(project.compositions[destination]?.content.tracks.map(track => track.id) ?? [])
    try { nativeClipMoveBatch(project, active, destination, selected, Math.round(seconds * 1_000_000), targets, () => { let id: string; do { id = `move-check-${n++}` } while (reserved.has(id)); reserved.add(id); return id }); return null }
    catch (error) { return error instanceof Error ? error.message : 'Не удалось перенести клипы' }
  }
  const unavailable = $derived(reason())
  const pasteReason = $derived.by(()=>{
    if (!project || !nestedClipboard.value) return 'Буфер клипов пуст'
    let serial=0
    try { const choices=Object.fromEntries([...clipboardTracks,...clipboardCaptionTracks].map(track=>[track.id,clipboardTargets[track.id] ?? ''])); const plan=prepareNativeClipboardPaste(project,nestedClipboard.value,destination,Math.round(seconds*1000000),choices,()=>`paste-check-${serial++}`); pasteNativeCaptionTracks(plan.base,destination,plan.captions,plan.captionDeltaUs,choices,()=>`paste-caption-check-${serial++}`); return null }
    catch(error) { return error instanceof Error ? error.message : String(error) }
  })
  const crossCopyReason = $derived.by(() => {
    if (!project) return 'Нет открытого проекта'
    let serial = 0
    try { prepareNativeClipCopy(project, active, destination, selected, Math.round(seconds*1000000), targets, () => `copy-check-${serial++}`); return null }
    catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  const trimReason = $derived.by(() => {
    if (!project || !selected.length || trimDelta === 0) return 'Выберите клипы и задайте сдвиг правого края'
    try { preflightNativeClipTrim(project, active, selected, Math.round(trimDelta * 1000000), 'end'); return null } catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  const leftReason = $derived.by(() => {
    if (!project || !selected.length || leftDelta === 0) return 'Выберите клипы и задайте сдвиг левого края'
    try { preflightNativeClipTrim(project, active, selected, Math.round(leftDelta * 1000000), 'start'); return null } catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  const linkReason = $derived.by(() => {
    if (!project) return 'Нет открытого проекта'
    try { linkNativeClips(project, active, selected); return null } catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  const unlinkReason = $derived.by(() => {
    if (!project || !selected.length) return 'Выберите связанные клипы'
    try { return unlinkNativeClips(project, active, selected) === project ? 'Выбранные клипы не связаны' : null } catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  function replacementPlan() {
    if (!project || !selected.length) throw new Error('Выберите клипы для замены')
    const entry = library.library.find(entry => entry.id === replacementSourceId && entry.kind === 'source')
    if (!entry || !entry.url.trim()) throw new Error('Выберите исходный файл из медиатеки')
    const source = compositionSourceFromLibraryEntry(entry)
    replaceNativeMedia(project, active, selected, source, keepOriginal, changeSourceStart ? Math.round(replacementStart * 1000000) : undefined)
    return source
  }
  const replacementReason = $derived.by(() => { try { replacementPlan(); return null } catch (error) { return error instanceof Error ? error.message : String(error) } })
  function replace(): void {
    try { void replaceNestedMedia(active, selected, replacementPlan(), keepOriginal, changeSourceStart ? Math.round(replacementStart * 1000000) : undefined) } catch (error) { nestedHistory.error = error instanceof Error ? error.message : String(error) }
  }
  async function uploadReplacement(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    if (!file) return
    if (disabled || nestedHistory.busy || uploadBusy || !project || !selected.length) { input.value = ''; return }
    const identity = JSON.stringify(project), target = active, members = [...selected], selection = JSON.stringify(selected), saveOriginal = keepOriginal, sourceStart = changeSourceStart ? Math.round(replacementStart * 1000000) : undefined
    uploadBusy = true; nestedHistory.error = ''
    try {
      const media = await uploadFile(file)
      void loadLibrary()
      if (!alive) return
      if (nestedHistory.busy) throw new Error('Во время загрузки началась другая правка. Выберите загруженный файл в медиатеке.')
      if (active !== target || JSON.stringify(project) !== identity || JSON.stringify(selected) !== selection) throw new Error('Проект или выбор изменился во время загрузки. Выберите загруженный файл в медиатеке.')
      if (typeof media.url !== 'string' || !media.url.trim()) throw new Error('У загруженного файла нет доступного адреса')
      const source = compositionSourceFromLibraryEntry({ ...media, kind: 'source', createdAt: Date.now()/1000 })
      await replaceNestedMedia(target, members, source, saveOriginal, sourceStart)
    } catch (error) { if (alive) nestedHistory.error = error instanceof Error ? error.message : String(error) }
    finally { if (alive) uploadBusy = false; input.value = '' }
  }
  function versionReason(copy: boolean): string | null {
    if (!project || !selected.length) return 'Выберите клипы'
    try { return setNativeMediaVersion(project, active, selected, copy) === project ? 'Эта версия уже используется' : null } catch (error) { return error instanceof Error ? error.message : String(error) }
  }
  const originalReason = $derived(versionReason(false))
  const copyReason = $derived(versionReason(true))
  function duplicationReason(startUs?: number): string | null {
    if (!project || !selected.length) return 'Выберите клипы для копирования'
    const used = new SvelteSet([...tracks.flatMap(track => track.clips.map(clip => clip.id)), ...(project.compositions[active]?.content.captionTracks ?? []).flatMap(track => track.clips.map(clip => clip.id)), ...tracks.flatMap(track => track.kind === 'video' ? track.transitions.map(transition => transition.id) : [])])
    let serial = 0
    try { duplicateNativeClips(project, active, selected, () => { let id: string; do { id = `duplicate-check-${serial++}` } while (used.has(id)); used.add(id); return id }, startUs); return null } catch (error) { return error instanceof Error ? error.message : String(error) }
  }
  const duplicateReason = $derived(duplicationReason())
  const positionedCopyReason = $derived(duplicationReason(Math.round(copyStartSeconds * 1000000)))
  async function duplicate(startUs?: number): Promise<void> {
    const target = active
    const copies = await duplicateNestedClips(target, selected, startUs)
    if (alive && active === target && copies.length) selected = [...copies]
  }
  async function systemClipboard(read: boolean): Promise<void> {
    if (disabled || nestedHistory.busy || uploadBusy || systemClipboardBusy) return
    const document=project, previous=nestedClipboard.value
    systemClipboardBusy=true;systemClipboardStatus='';systemClipboardError=''
    try {
      if (read) {
        if (!navigator.clipboard?.readText) throw new Error('Браузер не поддерживает чтение системного буфера')
        const text=await navigator.clipboard.readText()
        if (!alive) return
        if (project!==document || nestedClipboard.value!==previous || nestedHistory.busy) throw new Error('Проект или буфер изменился. Повторите чтение.')
        if (await importNestedClipboardText(text)) {if(alive)systemClipboardStatus='Клипы получены. Выберите место и вставьте их из буфера.'}
      } else {
        if (!previous) throw new Error('Сначала скопируйте клипы в буфер редактора')
        if (!navigator.clipboard?.writeText) throw new Error('Браузер не поддерживает запись в системный буфер')
        await navigator.clipboard.writeText(serializeNativeClipboard(previous))
        if (alive) systemClipboardStatus='Клипы скопированы для другого окна Video Kadr.'
      }
    } catch(error) {if(alive)systemClipboardError=error instanceof DOMException && error.name==='NotAllowedError'?'Браузер не разрешил доступ к системному буферу.':error instanceof Error?error.message:'Не удалось обратиться к системному буферу'}
    finally {systemClipboardBusy=false}
  }
  async function keepKeyboardFocus(operation: () => Promise<void>): Promise<void> {
    const scope=keyboardScope, before=document.activeElement, owned=!!scope && !!before && scope.contains(before)
    await operation()
    if (!alive || !owned || !scope?.isConnected) return
    const current=document.activeElement
    if (current===document.body || (current===before && before instanceof HTMLButtonElement && before.disabled)) scope.focus({preventScroll:true})
  }
  function nativeKey(event: KeyboardEvent): void {
    if (event.defaultPrevented || event.isComposing || shortcutState.dialogOpen || shortcutState.capturingCommandId) return
    const target=event.target
    if (!(target instanceof Element) || !keyboardScope?.contains(target)) return
    if (target.closest('textarea,select,[contenteditable]:not([contenteditable="false"]),[role="textbox"]') || (target instanceof HTMLInputElement && !['checkbox','radio','button'].includes(target.type))) return
    const command=shortcutCommandForEvent('native',event)
    if (!command) return
    event.preventDefault();event.stopPropagation()
    if (disabled || nestedHistory.busy || uploadBusy || systemClipboardBusy || event.repeat) return
    const members=[...selected,...captionSelected]
    if (command==='native.copy' && members.length) void copyNestedSelectionToClipboard(active,members)
    else if (command==='native.cut' && members.length) void keepKeyboardFocus(()=>cutNestedSelectionToClipboard(active,members))
    else if (command==='native.delete' && members.length) void keepKeyboardFocus(()=>removeNestedClips(active,members))
    else if (command==='native.undo') void keepKeyboardFocus(async()=>undoNestedComposition())
    else if (command==='native.redo') void keepKeyboardFocus(async()=>redoNestedComposition())
    else if (command==='native.paste' && nestedClipboard.value) {
      const choices=Object.fromEntries([...clipboardTracks,...clipboardCaptionTracks].map(track=>[track.id,destination===active ? clipboardTargets[track.id] ?? '' : '']))
      void keepKeyboardFocus(()=>pasteNestedClipboard(active,nestedComposition.timeUs,choices))
    }
  }
  $effect(()=>listenMany([[window,'keydown',eventListener(nativeKey),true]]))
  function transfer(): void {
    if (!project) return
    try { void moveNestedClips(nativeClipMoveBatch(project, active, destination, selected, Math.round(seconds * 1_000_000), targets, () => `track-${crypto.randomUUID()}`)) }
    catch (error) { nestedHistory.error = error instanceof Error ? error.message : String(error) }
  }
</script>
{#if project}
  <fieldset tabindex="-1" bind:this={keyboardScope} data-native-clipboard class="composition-fieldset" disabled={disabled || nestedHistory.busy || uploadBusy || systemClipboardBusy}>
    <legend>Клипы таймлайна</legend>
    <p>Горячие клавиши действуют внутри этой панели: копировать {shortcutLabel('native.copy')}, вырезать {shortcutLabel('native.cut')}, вставить {shortcutLabel('native.paste')}. Вставка клавишами идёт на курсор активного таймлайна.</p>
    {#each tracks as track (track.id)}
      {#each track.clips as clip (clip.id)}
        <label><input type="checkbox" data-clip-id={clip.id} disabled={track.locked} checked={selected.includes(clip.id)} onchange={(event) => select(clip.id, event.currentTarget.checked)} /> {track.name} · {clipTitle(clip, track.kind)} · {(nativeClipStart(clip)/1000000).toFixed(3)}–{(nativeClipEnd(clip)/1000000).toFixed(3)} с</label>
        {#if track.kind==='text'&&'text' in clip&&project.compositions[active]?.content.metadata?.captionTextTracks?.includes(track.id)}
          <NestedCaptionTextEditor compositionId={active} {clip} disabled={disabled||track.locked} />
        {/if}
      {/each}
    {/each}
    {#each captionTracks as track (track.id)}
      {#each track.clips as cue (cue.id)}
        <label><input type="checkbox" data-caption-id={cue.id} disabled={track.locked} checked={captionSelected.includes(cue.id)} onchange={event=>{captionSelected=event.currentTarget.checked?[...captionSelected,cue.id]:captionSelected.filter(id=>id!==cue.id)}} /> {track.name} · {cue.text.slice(0,40)} · {captionDisplaySeconds(cue.timelineStartTick).toFixed(3)}–{captionDisplaySeconds(cue.timelineEndTick).toFixed(3)} с</label>
      {/each}
    {/each}
    <label>В таймлайн<select aria-label="Таймлайн для переноса клипов" value={destination} onchange={(event) => { destination = event.currentTarget.value; targets = {} }}>
      {#each orderedTimelineIds(project) as id (id)}<option value={id}>{project.compositions[id]?.content.metadata?.sequenceName || id}</option>{/each}
    </select></label>
    <label>Начало, с<input aria-label="Начало переноса клипов" type="number" min="0" step="0.001" value={seconds} oninput={(event) => { seconds = event.currentTarget.valueAsNumber }} /></label>
    {#each tracks.filter(track => track.clips.some(clip => selected.includes(clip.id))) as track (track.id)}
      <label>{track.name} → дорожка<select aria-label={`Целевая дорожка ${track.id}`} value={targets[track.id] ?? defaultNativeTargetTrack(project, active, destination, track.id)} onchange={(event) => { targets = { ...targets, [track.id]: event.currentTarget.value } }}>
        {#each project.compositions[destination]?.content.tracks.filter(item => item.kind === track.kind) ?? [] as target (target.id)}<option value={target.id} disabled={target.locked}>{target.name}</option>{/each}
        <option value="">Новая дорожка</option>
      </select></label>
    {/each}
    <p>Связанные клипы выбираются вместе. Для несвязанных видео и звука выберите оба клипа, чтобы сохранить синхронизацию.</p>
    <p>Для копирования эффекта с другим слоем выберите также используемые в этом участке клипы управляющей дорожки. Для переноса в другой таймлайн выберите всю управляющую дорожку.</p>
    <button class="btn ghost sm" type="button" disabled={disabled || nestedHistory.busy || unavailable !== null} title={unavailable ?? undefined} onclick={transfer}>Перенести выбранные клипы</button>
    <button class="btn ghost sm" type="button" disabled={!selected.length && !captionSelected.length} onclick={()=>void copyNestedSelectionToClipboard(active,[...selected,...captionSelected])}>Копировать в буфер</button>
    <button class="btn ghost sm" type="button" disabled={!selected.length && !captionSelected.length} onclick={()=>void keepKeyboardFocus(()=>cutNestedSelectionToClipboard(active,[...selected,...captionSelected]))}>Вырезать в буфер</button>
    <button class="btn ghost sm" type="button" onclick={()=>void systemClipboard(true)}>Прочитать системный буфер</button>
    <button class="btn ghost sm" type="button" disabled={!clipboardCount} onclick={()=>void systemClipboard(false)}>Передать буфер другому окну</button>
    <p>Передаются клипы, оформление и ссылки на медиатеку. Для воспроизведения исходные файлы должны быть доступны в другом окне.</p>
    {#if systemClipboardBusy}<p role="status">Обращаюсь к системному буферу…</p>{/if}
    {#if systemClipboardStatus}<p role="status">{systemClipboardStatus}</p>{/if}
    {#if systemClipboardError}<p role="alert">{systemClipboardError}</p>{/if}
    {#if clipboardCount}
      <p role="status">В буфере клипов: {clipboardCount}. Вставка использует выбранный таймлайн и время начала.</p>
      {#each clipboardTracks as track (track.id)}
        <label>{track.name} из буфера → дорожка<select aria-label={`Дорожка для вставки ${track.id}`} value={clipboardTargets[track.id] ?? ''} onchange={event=>{clipboardTargets={...clipboardTargets,[track.id]:event.currentTarget.value}}}>
          <option value="">Новая дорожка</option>
          {#each project.compositions[destination]?.content.tracks.filter(item=>item.kind===track.kind) ?? [] as target (target.id)}<option value={target.id} disabled={target.locked}>{target.name}</option>{/each}
        </select></label>
      {/each}
      {#each clipboardCaptionTracks as track (track.id)}
        <label>{track.name} из буфера → субтитры<select aria-label={`Дорожка субтитров для вставки ${track.id}`} value={clipboardTargets[track.id] ?? ''} onchange={event=>{clipboardTargets={...clipboardTargets,[track.id]:event.currentTarget.value}}}>
          <option value="">Новая дорожка субтитров</option>
          {#each project.compositions[destination]?.content.captionTracks ?? [] as target (target.id)}<option value={target.id} disabled={target.locked}>{target.name}</option>{/each}
        </select></label>
      {/each}
      <button class="btn ghost sm" type="button" disabled={pasteReason !== null} title={pasteReason ?? undefined} onclick={()=>void pasteNestedClipboard(destination,Math.round(seconds*1000000),Object.fromEntries([...clipboardTracks,...clipboardCaptionTracks].map(track=>[track.id,clipboardTargets[track.id] ?? ''])))}>Вставить из буфера</button>
      <button class="btn ghost sm" type="button" onclick={()=>{nestedClipboard.value=null}}>Очистить буфер клипов</button>
    {/if}
    <button class="btn ghost sm" type="button" disabled={crossCopyReason !== null} title={crossCopyReason ?? undefined} onclick={() => void copyNestedClips(active, destination, selected, Math.round(seconds*1000000), targets)}>Копировать в выбранный таймлайн</button>
    <button class="btn ghost sm" type="button" disabled={disabled || nestedHistory.busy || selected.length < 2} onclick={() => void keepKeyboardFocus(()=>mergeNestedCaptionClips(active,selected))}>Объединить субтитры вложенной композиции</button>
    <button class="btn ghost sm" type="button" disabled={disabled || nestedHistory.busy || (!selected.length && !captionSelected.length)} onclick={() => void keepKeyboardFocus(()=>removeNestedClips(active, [...selected,...captionSelected]))}>Удалить выбранные клипы</button>
    <button class="btn ghost sm" type="button" disabled={linkReason !== null} title={linkReason ?? undefined} onclick={() => void editNestedClipLinks(active, selected)}>Связать выбранные клипы</button>
    <button class="btn ghost sm" type="button" disabled={rippleBlocked||!selected.length} onclick={()=>void keepKeyboardFocus(()=>rippleNestedComposition(active,{clipIds:selected}))}>Удалить выбранные со сдвигом</button>
    <button class="btn ghost sm" type="button" disabled={rippleBlocked||raiseReason!==null} title={raiseReason??undefined} onclick={()=>raiseSelection(false)}>На новую дорожку с сохранением времени</button>
    <button class="btn ghost sm" type="button" disabled={rippleBlocked||raiseReason!==null} title={raiseReason??undefined} onclick={()=>raiseSelection(true)}>На новую дорожку на курсор</button>
    <label>Клип для сдвига материала <select aria-label="Клип для сдвига материала" value={slipLocation?.clip.id??''} onchange={event=>slipClipId=event.currentTarget.value} disabled={!slipRows.length}>
      {#each slipRows as row (row.clip.id)}<option value={row.clip.id}>{clipTitle(row.clip,row.track.kind)}</option>{/each}
    </select></label>
    <label>Сдвиг материала, с <input aria-label="Сдвиг материала внутри клипа" type="number" step="0.001" bind:value={slipSeconds}/></label>
    <button class="btn ghost sm" type="button" disabled={rippleBlocked||slipReason!==null||!Number.isFinite(slipSeconds)||slipSeconds===0} title={slipReason??undefined} onclick={()=>slipLocation&&void keepKeyboardFocus(()=>slipNestedLayer(active,slipLocation.clip.id,Math.round(slipSeconds*1000000)))}>Сдвинуть материал внутри клипа</button>
    <label>Дорожка промежутка <select aria-label="Дорожка для закрытия промежутка" value={rippleLane?.id??''} onchange={event=>rippleTrack=event.currentTarget.value}>
      {#each tracks as track (track.id)}<option value={track.id}>{track.name}</option>{/each}
    </select></label>
    <button class="btn ghost sm" type="button" disabled={rippleBlocked||!rippleGap||!rippleLane||rippleLane.locked} onclick={()=>rippleGap&&rippleLane&&void keepKeyboardFocus(()=>rippleNestedComposition(active,{trackId:rippleLane.id,start:rippleGap.start,end:rippleGap.end}))}>Закрыть промежуток на курсоре</button>
    <button class="btn ghost sm" type="button" disabled={unlinkReason !== null} title={unlinkReason ?? undefined} onclick={() => void editNestedClipLinks(active, selected, true)}>Разорвать связь клипов</button>
    <button class="btn ghost sm" type="button" disabled={duplicateReason !== null} title={duplicateReason ?? undefined} onclick={() => void duplicate()}>Копировать после выбранных</button>
    <label>Начало копии, с <input aria-label="Начало копии клипов" type="number" min="0" step="0.001" bind:value={copyStartSeconds} /></label>
    <button class="btn ghost sm" type="button" disabled={positionedCopyReason !== null} title={positionedCopyReason ?? undefined} onclick={() => void duplicate(Math.round(copyStartSeconds * 1000000))}>Копировать в заданное время</button>
    <NestedSourceKeyMarks marks={sourceKeyGroups} disabled={rippleBlocked} compositionId={active} clipIds={selected} rangeStart={sourceKeyRange[0]} rangeEnd={sourceKeyRange[1]} />
    <button class="btn ghost sm" type="button" disabled={rippleBlocked||!project||concatKeyTargetFromMarks(sourceKeyMarks,nestedComposition.timeUs,-1)===null} onclick={()=>void stepNestedConcatKey(active,selected,-1)}>Предыдущий исходный ключ</button>
    <button class="btn ghost sm" type="button" disabled={rippleBlocked||!project||concatKeyTargetFromMarks(sourceKeyMarks,nestedComposition.timeUs,1)===null} onclick={()=>void stepNestedConcatKey(active,selected,1)}>Следующий исходный ключ</button>
    <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||!selected.length} onclick={()=>void clearNestedConcatKeys(active,selected)}>Очистить исходные ключи клипа и Adjust</button>
    <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||!selected.length} onclick={()=>void quickPoseNestedClips(active,selected,'rotate90')}>Повернуть выбранные на 90°</button>
    <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||!selected.length} onclick={()=>void quickPoseNestedClips(active,selected,'flipH')}>Отразить выбранные по горизонтали</button>
    <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||!selected.length} onclick={()=>void quickPoseNestedClips(active,selected,'flipV')}>Отразить выбранные по вертикали</button>
    <button class="btn ghost sm" type="button" disabled={disabled || nestedHistory.busy || !freezeTarget || !nestedComposition.projectId || nestedComposition.revision===null || !authState.token} onclick={()=>void freezeAtCursor()}>Стоп-кадр по курсору</button>
    <button class="btn ghost sm" type="button" disabled={!selected.length} onclick={() => void splitNestedClips(active, selected, nestedComposition.timeUs)}>Разрезать выбранные на курсоре</button>
    <label>Новый исходный файл <select aria-label="Новый источник выбранных клипов" value={replacementSourceId} onchange={event => { replacementSourceId = event.currentTarget.value }}><option value="">Выберите файл</option>{#each library.library.filter(entry => entry.kind === 'source') as entry (entry.id)}<option value={entry.id}>{entry.title?.trim() || entry.filename}</option>{/each}</select></label>
    <label><input type="checkbox" aria-label="Изменить начало в новом файле" bind:checked={changeSourceStart} /> Задать начало диапазона в новом файле</label>
    {#if changeSourceStart}<p>Начало задаётся для первого выбранного клипа. Диапазоны остальных сдвигаются на столько же.</p><label>Начало диапазона в файле, с <input aria-label="Начало диапазона для замены" type="number" min="0" step="0.001" bind:value={replacementStart} /></label>{/if}
    <label><input type="checkbox" aria-label="Сохранить оригинал выбранных клипов" bind:checked={keepOriginal} /> Сохранить оригинал для сравнения</label>
    <button class="btn ghost sm" type="button" disabled={replacementReason !== null} title={replacementReason ?? undefined} onclick={replace}>Заменить источник выбранных клипов</button>
    <button class="btn ghost sm" type="button" disabled={originalReason !== null} title={originalReason ?? undefined} onclick={() => void switchNestedMediaVersion(active, selected, false)}>Показать оригинал</button>
    <button class="btn ghost sm" type="button" disabled={copyReason !== null} title={copyReason ?? undefined} onclick={() => void switchNestedMediaVersion(active, selected, true)}>Показать обработанную копию</button>
    {#if project&&enhancementTarget}<ImageEnhancementPanel token={authState.token} projectId={nestedComposition.projectId??null} sourceId={enhancementTarget.source.id} targetKey={`${active}/${enhancementTarget.clipId}`} width={enhancementTarget.source.width} height={enhancementTarget.source.height} context={project} disabled={disabled||nestedHistory.busy} onaccepted={async entry=>{const target=enhancementTarget;if(!target||disabled||nestedHistory.busy)throw new Error('Выбор клипа изменился');await replaceNestedMedia(active,[target.clipId],compositionSourceFromLibraryEntry(entry),true);void loadLibrary()}} />{/if}
    {#if project&&enhancementTarget&&(!project.compositions[active]?.content.metadata?.mediaVersions?.[enhancementTarget.clipId]||project.compositions[active]?.content.metadata?.mediaVersions?.[enhancementTarget.clipId]?.cutout)}<NestedConcatCutout savedCutout={project.compositions[active]?.content.metadata?.mediaVersions?.[enhancementTarget.clipId]?.cutout} token={authState.token} compositionId={active} clipId={enhancementTarget.clipId} disabled={disabled||nestedHistory.busy}/>{/if}
    {#if project&&enhancementTarget}<ImageCutoutPanel projectMask={project.compositions[active]?.content.metadata?.mediaVersions?.[enhancementTarget.clipId]?.cutout?{clipId:enhancementTarget.clipId,compositionId:active,snapshot:project.compositions[active]!.content.metadata!.mediaVersions![enhancementTarget.clipId]!.cutout!}:undefined} token={authState.token} projectId={nestedComposition.projectId??null} sourceId={enhancementTarget.source.id} targetKey={`${active}/${enhancementTarget.clipId}`} width={enhancementTarget.source.width} height={enhancementTarget.source.height} context={project} disabled={disabled||nestedHistory.busy} onaccepted={async entry=>{const target=enhancementTarget;if(!target||disabled||nestedHistory.busy)throw new Error('Выбор клипа изменился');await replaceNestedCutoutMedia(active,[target.clipId],entry);void loadLibrary()}} />{/if}
    {#if project&&videoEnhancementTarget&&!project.compositions[active]?.content.metadata?.mediaVersions?.[videoEnhancementTarget.clipId]}<NestedConcatVideoCutout token={authState.token} compositionId={active} clipId={videoEnhancementTarget.clipId} disabled={disabled||nestedHistory.busy}/>{/if}
    {#if project&&videoEnhancementTarget}<VideoEnhancementPanel token={authState.token} projectId={nestedComposition.projectId??null} sourceId={videoEnhancementTarget.source.id} targetKey={`${active}/${videoEnhancementTarget.members.join('/')}`} width={videoEnhancementTarget.source.width} height={videoEnhancementTarget.source.height} context={project} disabled={disabled||nestedHistory.busy} onaccepted={async entry=>{const target=videoEnhancementTarget;if(!target||disabled||nestedHistory.busy)throw new Error('Выбор клипа изменился');await replaceNestedMedia(active,[target.clipId],compositionSourceFromLibraryEntry(entry),true);void loadLibrary()}} />{/if}
    {#if project&&videoEnhancementTarget}<PersonVideoPanel ontracked={async(entry,expected)=>{const target=videoEnhancementTarget;if(!target||disabled||nestedHistory.busy)throw new Error('Выбор клипа изменился');await attachNestedVideoRegion(active,target.clipId,expected,entry);void loadLibrary()}} savedMask={project.compositions[active]?.content.metadata?.mediaVersions?.[videoEnhancementTarget.clipId]?.videoCutout?{clipId:videoEnhancementTarget.clipId,compositionId:active,snapshot:project.compositions[active]!.content.metadata!.mediaVersions![videoEnhancementTarget.clipId]!.videoCutout!}:undefined} token={authState.token} projectId={nestedComposition.projectId??null} sourceId={videoEnhancementTarget.source.id} targetKey={`${active}/${videoEnhancementTarget.members.join('/')}`} width={videoEnhancementTarget.source.width} height={videoEnhancementTarget.source.height} context={project} disabled={disabled||nestedHistory.busy} onaccepted={async entry=>{const target=videoEnhancementTarget;if(!target||disabled||nestedHistory.busy)throw new Error('Выбор клипа изменился');await replaceNestedVideoCutoutMedia(active,[target.clipId],entry);void loadLibrary()}} />{/if}
    <label>Заменить файлом с компьютера <input aria-label="Файл для замены выбранных клипов" type="file" accept="video/*,audio/*,image/*" disabled={!selected.length} onchange={event => void uploadReplacement(event)} /></label>
    {#if uploadBusy}<p role="status">Загружаю файл…</p>{/if}
    <NestedSourceSpeedCurve {project} compositionId={active} clipIds={selected} disabled={disabled || nestedHistory.busy} />
    <label>Сдвиг левого края, с <input aria-label="Сдвиг левого края клипов" type="number" step="0.001" bind:value={leftDelta} /></label>
    <button class="btn ghost sm" type="button" disabled={leftReason !== null} title={leftReason ?? undefined} onclick={() => void trimNestedClipStarts(active, selected, Math.round(leftDelta * 1000000))}>Изменить левый край</button>
    <label>Сдвиг правого края, с <input aria-label="Сдвиг правого края клипов" type="number" step="0.001" bind:value={trimDelta} /></label>
    <button class="btn ghost sm" type="button" disabled={trimReason !== null} title={trimReason ?? undefined} onclick={() => void trimNestedClipEnds(active, selected, Math.round(trimDelta * 1000000))}>Изменить правый край</button>
  </fieldset>
{/if}
