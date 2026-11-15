<script lang="ts">
  import { nestedComposition, nestedHistory, importNestedCaptionFile, editNestedSubtitleDocuments } from '$lib/state/nestedComposition.svelte'
  import { nestedCaptionExportCues, formatSubtitleExport, type SubtitleExportFormat } from '$lib/subtitles/export'
  import { MAX_SRT_BYTES } from '$lib/subtitles/srt'
  import SubtitleFilePicker from './SubtitleFilePicker.svelte'
  import CaptionFileExport from './CaptionFileExport.svelte'
  import { authState } from '$lib/state/auth.svelte'
  let error = $state(''), message = $state(''), importing = $state(false), atPlayhead = $state(false)
  const documents = $derived(nestedComposition.document?.compositions[nestedComposition.document.root]?.content.metadata?.subtitleDocuments)
  const activeId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root)
  const content = $derived(activeId ? nestedComposition.document?.compositions[activeId]?.content : undefined)
  const fileTracks = $derived(content?.captionTracks ?? [])
  let trackId = $state('')
  const selectedTrack = $derived(fileTracks.find(track => track.id === trackId) ?? fileTracks[0])
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
      <CaptionFileExport composition={{ canvas: { fps: content.canvas.fpsMilli / 1000, fpsRate: content.canvas.fpsRate },
        captionTracks: fileTracks }} activeId={selectedTrack.id} />
    {/key}
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>
