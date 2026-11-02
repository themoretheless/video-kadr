<script lang="ts">
  import { nestedComposition, nestedHistory, importNestedCaptionFile } from '$lib/state/nestedComposition.svelte'
  import { nestedCaptionExportCues, formatSubtitleExport, type SubtitleExportFormat } from '$lib/subtitles/export'
  import { MAX_SRT_BYTES } from '$lib/subtitles/srt'
  let error = $state(''), message = $state(''), importing = $state(false), atPlayhead = $state(false)
  async function importFile(event:Event) {
    const input=event.currentTarget as HTMLInputElement,file=input.files?.[0]
    input.value=''
    if(!file)return
    error='';message='';importing=true
    const project=nestedComposition.document,identity=JSON.stringify(project)
    const id=nestedComposition.activeCompositionId??project?.root
    const offset=atPlayhead?nestedComposition.timeUs:0
    try {
      if(!id||!project)throw new Error('Сначала откройте вложенный проект')
      if(file.size>MAX_SRT_BYTES)throw new Error('Файл субтитров превышает лимит 2 МиБ')
      const body=await file.text()
      if(JSON.stringify(nestedComposition.document)!==identity||(nestedComposition.activeCompositionId??project.root)!==id)throw new Error('Проект или композиция изменились во время чтения файла')
      if(!await importNestedCaptionFile(id,body,offset))throw new Error(nestedHistory.error||'Импорт отменён: проект изменился')
      message='Субтитры импортированы'
    } catch(cause){error=cause instanceof Error?cause.message:String(cause)}
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
  <label>Импорт SRT / VTT / ASS / TXT<input type="file" aria-label="Импорт субтитров вложенной композиции" accept=".srt,.vtt,.ass,.ssa,.txt" disabled={importing||nestedHistory.busy} onchange={event=>void importFile(event)} /></label>
  {#if message}<p role="status">{message}</p>{/if}
  <button class="btn ghost sm" type="button" onclick={() => download('srt')}>Субтитры активной композиции · SRT</button>
  <button class="btn ghost sm" type="button" onclick={() => download('vtt')}>Субтитры активной композиции · VTT</button>
  {#if error}<p role="alert">{error}</p>{/if}
</section>
