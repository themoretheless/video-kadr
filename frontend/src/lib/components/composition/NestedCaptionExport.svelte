<script lang="ts">
  import { nestedComposition, nestedHistory, importNestedCaptionFile, importPreparedNestedCaptionFile, editNestedSubtitleDocuments } from '$lib/state/nestedComposition.svelte'
  import { prepareCaptionFileImport } from '$lib/api'
  import { readCaptionFileBase64 } from '$lib/composition/captionFileImport'
  import { nestedCaptionExportCues, formatSubtitleExport, type SubtitleExportFormat } from '$lib/subtitles/export'
  import { MAX_SRT_BYTES } from '$lib/subtitles/srt'
  import SubtitleFilePicker from './SubtitleFilePicker.svelte'
  import CaptionFileExport from './CaptionFileExport.svelte'
  import { authState } from '$lib/state/auth.svelte'
  let error = $state(''), message = $state(''), importing = $state(false), atPlayhead = $state(false)
  let draggingTrack = $state(false)
  const trackFormats = ['.srt', '.vtt', '.scc', '.mcc', '.stl', '.ttml', '.dfxp']
  const documents = $derived(nestedComposition.document?.compositions[nestedComposition.document.root]?.content.metadata?.subtitleDocuments)
  const activeId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root)
  const content = $derived(activeId ? nestedComposition.document?.compositions[activeId]?.content : undefined)
  const fileTracks = $derived(content?.captionTracks ?? [])
  let trackId = $state('')
  let disposed = false
  $effect(() => () => { disposed = true })
  const selectedTrack = $derived(fileTracks.find(track => track.id === trackId) ?? fileTracks[0])
  function selectTrack(files: FileList | null | undefined, dropped = false) {
    if (!content || importing || nestedHistory.busy || !files?.length) return
    error = ''; message = ''
    if (files.length !== 1) {
      error = 'Выберите один файл субтитров'
      return
    }
    const file = files[0]!
    if (dropped && !trackFormats.some(extension => file.name.toLowerCase().endsWith(extension))) {
      error = 'Выберите файл SRT, VTT, SCC, MCC, STL, TTML или DFXP'
      return
    }
    void importTrack(file)
  }

  async function importTrack(file: File) {
    const project = nestedComposition.document, id = activeId
    if (!project || !id || !content) return
    const snapshot = JSON.stringify(project), owner = authState.token
    const offset = atPlayhead ? nestedComposition.timeUs : 0
    const { fpsMilli, fpsRate } = content.canvas
    const check = () => {
      if (disposed || authState.token !== owner || JSON.stringify(nestedComposition.document) !== snapshot || activeId !== id) {
        throw new Error('Проект или композиция изменились во время импорта')
      }
    }
    importing = true
    error = ''; message = ''
    try {
      const rate = fpsRate ?? { numerator: fpsMilli, denominator: 1000 }
      const encoded = await readCaptionFileBase64(file)
      check()
      const prepared = await prepareCaptionFileImport(encoded, file.name, { num: rate.numerator, den: rate.denominator })
      check()
      if (!await importPreparedNestedCaptionFile(id, prepared, offset, check)) {
        throw new Error(nestedHistory.error || 'Импорт дорожки отменён')
      }
      message = ['Дорожка субтитров импортирована', ...(prepared.document.warnings ?? [])].join('. ')
    } catch (cause) {
      if (!disposed) error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      importing = false
    }
  }
  async function importFile(file: File, dropTime?: number, saveSource = false): Promise<boolean> {
    if(importing||nestedHistory.busy)return false
    error='';message='';importing=true
    const project=nestedComposition.document,identity=JSON.stringify(project)
    const id=nestedComposition.activeCompositionId??project?.root
    const offset=dropTime ?? (atPlayhead ? nestedComposition.timeUs : 0)
    const owner=authState.token
    try {
      if(!id||!project)throw new Error('Сначала откройте вложенный проект')
      if(file.size>MAX_SRT_BYTES)throw new Error('Файл субтитров превышает лимит 2 МиБ')
      const body=await file.text()
      if(authState.token!==owner||JSON.stringify(nestedComposition.document)!==identity||(nestedComposition.activeCompositionId??project.root)!==id)throw new Error('Проект или композиция изменились во время чтения файла')
      let simplified = 0
      if(!await importNestedCaptionFile(id,body,offset,value=>{simplified=value}, saveSource ? { id: crypto.randomUUID(), name: file.name } : undefined))throw new Error(nestedHistory.error||'Импорт отменён: проект изменился')
      message=`Субтитры импортированы${simplified ? `. Неподдерживаемые эффекты ASS пропущены в ${simplified} репл.` : ''}`
      return true
    } catch(cause){error=cause instanceof Error?cause.message:String(cause);return false}
    finally{importing=false}
  }
  function download(format: SubtitleExportFormat) {
    error = ''
    try {
      const project = nestedComposition.document
      if (!project) throw new Error('Сначала откройте вложенный проект')
      const id = nestedComposition.activeCompositionId ?? project.root
      const cues = nestedCaptionExportCues(project,id)
      if (!cues.length) throw new Error('В выбранной композиции нет субтитров')
      const body = formatSubtitleExport(cues,format)
      const name = `${nestedComposition.name || 'subtitles'}-${id}`.replace(/[^\p{L}\p{N}._-]+/gu,'-').slice(0,100)
      const url = URL.createObjectURL(new Blob([body],{type:format === 'vtt' ? 'text/vtt;charset=utf-8' : 'application/x-subrip;charset=utf-8'}))
      const link = document.createElement('a')
      link.href = url; link.download = `${name}.${format}`
      document.body.append(link); link.click(); link.remove()
      setTimeout(()=>URL.revokeObjectURL(url),0)
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }
</script>
<section aria-label="Файлы субтитров вложенной композиции">
  <label><input type="checkbox" bind:checked={atPlayhead} disabled={importing||nestedHistory.busy} />Начать субтитры в позиции курсора</label>
  <SubtitleFilePicker label="Импорт субтитров вложенной композиции" disabled={importing||nestedHistory.busy}
    project={nestedComposition.document} {documents} ondocumentschange={editNestedSubtitleDocuments} onfile={importFile} onerror={notice=>{message='';error=notice}} />
  <div role="group" aria-label="Область импорта на дорожку субтитров" class="track-drop" class:dragging={draggingTrack}
    ondragover={event => {
      const transfer = event.dataTransfer
      if (!transfer?.types.includes('Files')) return
      event.preventDefault()
      draggingTrack = !!content && !importing && !nestedHistory.busy
      transfer.dropEffect = draggingTrack ? 'copy' : 'none'
    }}
    ondragleave={event => {
      if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) draggingTrack = false
    }}
    ondrop={event => {
      event.preventDefault()
      draggingTrack = false
      selectTrack(event.dataTransfer?.files, true)
    }}>
    <label>Выберите или перетащите файл на дорожку субтитров
      <input type="file" aria-label="Импорт на дорожку субтитров" accept={trackFormats.join(',')}
        disabled={!content||importing||nestedHistory.busy} onchange={event=>{
          selectTrack(event.currentTarget.files)
          event.currentTarget.value = ''
        }} />
    </label>
  </div>
  {#if message}<p role="status">{message}</p>{/if}
  <button class="btn ghost sm" type="button" onclick={() => download('srt')}>Субтитры активной композиции · SRT</button>
  <button class="btn ghost sm" type="button" onclick={() => download('vtt')}>Субтитры активной композиции · VTT</button>
  {#if content && selectedTrack}
    <label>Дорожка для экспорта
      <select aria-label="Дорожка для экспорта" value={selectedTrack.id} onchange={event=>{trackId=event.currentTarget.value}}>
        {#each fileTracks as track (track.id)}<option value={track.id}>{track.name}</option>{/each}
      </select>
    </label>
    {#key `${activeId}/${selectedTrack.id}`}
      <CaptionFileExport composition={{ canvas: { fps: content.canvas.fpsMilli / 1000,
        fpsRate: content.canvas.fpsRate ?? { numerator: content.canvas.fpsMilli, denominator: 1000 } },
        captionTracks: fileTracks }} activeId={selectedTrack.id} />
    {/key}
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>


<style>
  .track-drop { outline: 1px dashed transparent; }
  .track-drop.dragging { outline-color: currentColor; }
</style>
