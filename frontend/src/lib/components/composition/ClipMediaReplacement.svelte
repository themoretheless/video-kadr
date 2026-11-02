<script lang="ts">
  import { compositionState, replaceCompositionClipMedia, setCompositionClipMediaVersion } from '$lib/state/composition.svelte'
  import { uploadFile } from '$lib/api'
  import { loadLibrary } from '$lib/state/store.svelte'
  import { state as libraryState } from '$lib/state/store.svelte'
  import { replaceCompositionClipFromLibrary, replaceClipLibraryMediaReason } from '$lib/state/composition.svelte'
  import { replaceClipMediaReason } from '$lib/composition/replaceMedia'
  import { findClipLocation } from '$lib/composition/commands'
  import VideoEnhancementPanel from './VideoEnhancementPanel.svelte'
  import PersonVideoPanel from './PersonVideoPanel.svelte'
  import {replaceCompositionEnhancedVideoFromLibrary,replaceCompositionVideoCutoutFromLibrary,setCompositionEnhancedVideoVersion} from '$lib/state/composition.svelte'
  import {hasEnhancedVideoGroup,switchEnhancedVideo} from '$lib/composition/enhancedVideoReplace'
  import {attachCompositionVideoRegionFromLibrary,replaceCompositionCutoutFromLibrary} from '$lib/state/composition.svelte'
  import ImageCutoutPanel from './ImageCutoutPanel.svelte'
  import ImageEnhancementPanel from './ImageEnhancementPanel.svelte'
  import {authState} from '$lib/state/auth.svelte'
  let { clipId }: { clipId: string } = $props()
  let chosen = $state('')
  let processed = $state(false)
  let error = $state('')
  let busy = $state(false)
  let alive = true
  $effect(() => () => { alive = false })
  const current = $derived(findClipLocation(compositionState.document, clipId).clip)
  const imageSource=$derived('sourceId' in current&&compositionState.document.sources[current.sourceId]?.kind==='image'?compositionState.document.sources[current.sourceId]:undefined)
  const videoSource=$derived('sourceId' in current&&compositionState.document.sources[current.sourceId]?.kind==='video'?compositionState.document.sources[current.sourceId]:undefined)
  const versions = $derived(compositionState.document.metadata?.mediaVersions?.[clipId])
  const usingCopy = $derived(Boolean(versions && 'sourceId' in current && current.sourceId === versions.copySourceId))
  const versionReason = $derived.by(()=>{if(!versions)return null;if(videoSource&&hasEnhancedVideoGroup(compositionState.document,clipId)){try{switchEnhancedVideo(compositionState.document,clipId,!usingCopy);return null}catch(reason){return reason instanceof Error?reason.message:String(reason)}}return replaceClipMediaReason(compositionState.document,clipId,usingCopy?versions.originalSourceId:versions.copySourceId)})
  function toggleVersion(copy: boolean): void {
    error = ''
    try { if(videoSource&&hasEnhancedVideoGroup(compositionState.document,clipId))setCompositionEnhancedVideoVersion(clipId,copy);else setCompositionClipMediaVersion(clipId, copy) }
    catch (e) { error = e instanceof Error ? e.message : String(e) }
  }
  const candidates = $derived(Object.values(compositionState.document.sources).filter(s => 'sourceId' in current && s.id !== current.sourceId))
  const libraryCandidates = $derived(libraryState.library.filter(entry => entry.kind === 'source' && !compositionState.document.sources[entry.id]))
  function candidateReason(id: string): string | null {
    const entry = libraryCandidates.find(e => e.id === id)
    return entry ? replaceClipLibraryMediaReason(clipId, entry) : replaceClipMediaReason(compositionState.document, clipId, id)
  }
  const reason = $derived(chosen ? candidateReason(chosen) : 'Выберите новый источник')
  async function upload(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    if (!file || busy) return
    const original = compositionState.document, target = clipId, saveOriginal = processed
    busy = true; error = ''
    try {
      const media = await uploadFile(file)
      void loadLibrary()
      if (!alive) return
      if (clipId !== target || compositionState.document !== original) throw new Error('Проект изменился во время загрузки. Выберите загруженный файл в медиатеке.')
      replaceCompositionClipFromLibrary(target, { ...media, kind: 'source', createdAt: Date.now() / 1000 }, saveOriginal)
      chosen = ''
    } catch (e) { if (alive) error = e instanceof Error ? e.message : String(e) }
    finally { if (alive) busy = false; input.value = '' }
  }
  function replace(): void {
    error = ''
    try {
      const entry = libraryCandidates.find(e => e.id === chosen)
      if (entry) replaceCompositionClipFromLibrary(clipId, entry, processed)
      else replaceCompositionClipMedia(clipId, chosen, undefined, processed)
      chosen = ''
    }
    catch (e) { error = e instanceof Error ? e.message : String(e) }
  }
</script>
<fieldset class="composition-fieldset" disabled={busy}>
  <legend>Медиа этого клипа</legend>
  {#if versions}
    <label title={versionReason ?? undefined}><input aria-label="Использовать обработанную копию" type="checkbox" checked={usingCopy} disabled={versionReason !== null} onchange={(event) => toggleVersion(event.currentTarget.checked)} /> Использовать обработанную копию</label>
  {/if}
  <label><input aria-label="Сохранить оригинал для сравнения" type="checkbox" checked={processed} onchange={(event) => { processed = event.currentTarget.checked }} /> Сохранить оригинал для сравнения</label>
  <label>Новый источник
    <select aria-label="Новый источник клипа" value={chosen} onchange={(event) => { chosen = event.currentTarget.value; error = '' }}>
      <option value="">Выберите источник</option>
      {#each candidates as source (source.id)}
        <option value={source.id} title={replaceClipMediaReason(compositionState.document, clipId, source.id) ?? undefined} disabled={replaceClipMediaReason(compositionState.document, clipId, source.id) !== null}>{compositionState.media[source.id]?.filename ?? source.id}</option>
      {/each}
      {#each libraryCandidates as entry (entry.id)}
        <option value={entry.id} title={candidateReason(entry.id) ?? undefined} disabled={candidateReason(entry.id) !== null}>{entry.filename} · медиатека</option>
      {/each}
    </select>
  </label>
  <button class="btn ghost sm" type="button" disabled={reason !== null} title={reason ?? 'Заменить медиа выбранного клипа'} onclick={replace}>Заменить медиа клипа</button>
  <label>Заменить файлом с компьютера
    <input aria-label="Файл для замены медиа клипа" type="file" accept="video/*,audio/*,image/*" onchange={(event) => void upload(event)} />
  </label>
  {#if busy}<p role="status">Загружаю файл…</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
</fieldset>
{#if imageSource}<ImageEnhancementPanel token={authState.token} projectId={compositionState.projectId} sourceId={imageSource.id} targetKey={clipId} width={imageSource.width} height={imageSource.height} context={compositionState.document} onaccepted={entry=>{replaceCompositionClipFromLibrary(clipId,entry,true);void loadLibrary()}} />{/if}
{#if imageSource}<ImageCutoutPanel projectMask={versions?.cutout?{clipId,snapshot:versions.cutout}:undefined} token={authState.token} projectId={compositionState.projectId} sourceId={imageSource.id} targetKey={clipId} width={imageSource.width} height={imageSource.height} context={compositionState.document} onaccepted={entry=>{replaceCompositionCutoutFromLibrary(clipId,entry);void loadLibrary()}} />{/if}

{#if videoSource}<VideoEnhancementPanel token={authState.token} projectId={compositionState.projectId} sourceId={videoSource.id} targetKey={clipId} width={videoSource.width} height={videoSource.height} context={compositionState.document} onaccepted={entry=>{replaceCompositionEnhancedVideoFromLibrary(clipId,entry);void loadLibrary()}} />{/if}
{#if videoSource}<PersonVideoPanel ontracked={async(entry,expected)=>{attachCompositionVideoRegionFromLibrary(clipId,expected,entry);void loadLibrary()}} savedMask={versions?.videoCutout?{clipId,snapshot:versions.videoCutout}:undefined} token={authState.token} projectId={compositionState.projectId} sourceId={videoSource.id} targetKey={clipId} width={videoSource.width} height={videoSource.height} context={compositionState.document} onaccepted={entry=>{replaceCompositionVideoCutoutFromLibrary(clipId,entry);void loadLibrary()}} />{/if}
<style>
 .composition-fieldset{grid-template-columns:minmax(0,1fr)}
 label{min-width:0}
 select,input[type='file']{box-sizing:border-box;min-width:0;max-width:100%;width:100%}
</style>
