import {attachCompositionVideoRegion} from '../../composition/videoRegionAttach'
import {replaceVideoCutoutMedia} from '../../composition/videoCutoutMedia'
import {replaceCutoutMedia} from '../../composition/cutoutMedia'
import {replaceEnhancedVideo,switchEnhancedVideo} from '../../composition/enhancedVideoReplace'
import { replaceClipMedia } from '../../composition/replaceMedia.js'
import { importPreferences } from '../importPreferences.svelte.js'
import { pairImportedVideoAudio } from '../../composition/importPair.js'
import {
  registerSource,
  findClipLocation
} from '../../composition/commands.js'
import {
  assertValidComposition
} from '../../composition/validation.js'
import {
  relinkCompositionSource as relinkCompositionDocumentSource
} from '../../composition/relink.js'
import {
  COMPOSITION_TIME_BASE
} from '../../composition/types.js'
import type {
  CompositionSource
} from '../../composition/types.js'
import type {
  MediaEntry,
  MediaInfo,
  MediaType
} from '../../types.js'
import {
  setEditorMode
} from './editor.svelte.js'
import {
  CompositionMedia,
  DEFAULT_CANVAS,
  addAudio,
  addImage,
  addVideo,
  cloneCompositionMedia,
  commitDocument,
  compositionState,
  evenDimension,
  isMediaEntry,
  positiveInteger,
  positiveNumber,
  makeId,
  scheduleAutosave,
  selectCompositionClip,
  validFps
} from './core.svelte.js'

export function addMediaInfoToComposition(media: MediaInfo, options: { pairAudio?: boolean } = {}): string {
  const kind = inferMediaType(media)
  const source = compositionSourceFromMediaInfo(media, kind)
  let document = compositionState.document
  if (!Object.hasOwn(document.sources, source.id)) document = registerSource(document, source)

  let clipId: string
  if (kind === 'video') {
    const hasVideo = document.tracks.some(
      (track) => track.kind === 'video' && track.clips.length > 0,
    )
    if (!hasVideo) {
      document = {
        ...document,
        canvas: {
          ...document.canvas,
          width: evenDimension(media.width, DEFAULT_CANVAS.width, 3840),
          height: evenDimension(media.height, DEFAULT_CANVAS.height, 2160),
          fps: validFps(media.fps),
          fpsRate: undefined,
        },
      }
      assertValidComposition(document)
    }
    const result = addVideo(document, source)
    document = result.document
    clipId = result.clipId
    if ((options.pairAudio ?? importPreferences.autoPairAudio) && source.hasAudio) document = pairImportedVideoAudio(document, clipId, { audioTrackId: makeId('audio-track'), audioClipId: makeId('audio-clip'), linkId: makeId('link') })
  } else if (kind === 'audio') {
    const result = addAudio(document, source)
    document = result.document
    clipId = result.clipId
  } else {
    const result = addImage(document, source)
    document = result.document
    clipId = result.clipId
  }

  const mediaBinding: CompositionMedia = { id: media.id, url: media.url, filename: media.filename, mediaType: kind, duration: media.duration, width: media.width, height: media.height, fps: media.fps ?? null, vcodec: media.vcodec ?? null, acodec: media.acodec ?? null }
  commitDocument(document, { ...compositionState.media, [media.id]: mediaBinding })
  const location = findClipLocation(compositionState.document, clipId)
  selectCompositionClip(location.track.id, clipId)
  setEditorMode('composition')
  return clipId
}

/**
 * Atomically publish a completed microphone recording as an audio source and
 * place its clip at the current playhead. A malformed upload response cannot
 * create a video/image clip or leave a half-registered media binding behind.
 */

export function addVoiceoverMediaInfoToComposition(media: MediaInfo): string {
  if (media.mediaType !== 'audio') {
    throw new Error('Загруженная голосовая запись не распознана как аудио')
  }
  const uploadedSource = compositionSourceFromMediaInfo(media, 'audio')
  const existingSource = compositionState.document.sources[uploadedSource.id]
  if (existingSource && existingSource.kind !== 'audio') {
    throw new Error('Идентификатор голосовой записи уже занят другим типом медиа')
  }
  const source = existingSource ?? uploadedSource
  let document = compositionState.document
  if (!existingSource) document = registerSource(document, source)
  const result = addAudio(document, source)
  const mediaBinding: CompositionMedia = {
    id: source.id,
    url: media.url,
    filename: media.filename,
    mediaType: 'audio',
    duration: source.durationTicks / COMPOSITION_TIME_BASE,
    width: 0,
    height: 0,
    fps: media.fps ?? source.fps,
    vcodec: media.vcodec ?? source.vcodec,
    acodec: media.acodec ?? source.acodec,
  }
  commitDocument(result.document, { ...compositionState.media, [source.id]: mediaBinding })
  const location = findClipLocation(compositionState.document, result.clipId)
  selectCompositionClip(location.track.id, result.clipId)
  setEditorMode('composition')
  return result.clipId
}


export function addLibraryEntryToComposition(entry: MediaEntry): string {
  if (entry.kind !== 'source') throw new Error('В композицию можно добавить только исходный медиафайл')
  const mediaType = inferMediaType(entry)
  return addMediaInfoToComposition({
    id: entry.id,
    url: entry.url,
    filename: entry.filename,
    mediaType,
    duration: mediaType === 'image' ? 0 : positiveNumber(entry.duration),
    width: mediaType === 'audio' ? 0 : positiveInteger(entry.width),
    height: mediaType === 'audio' ? 0 : positiveInteger(entry.height),
    title: entry.title,
    fps: entry.fps,
    vcodec: entry.vcodec,
    acodec: entry.acodec,
  })
}


export function syncCompositionLibrary(entries: readonly MediaEntry[], authoritative = false): void {
  let changed = false
  const media = { ...compositionState.media }
  const availableSourceIds: string[] = []
  for (const entry of entries) {
    if (entry.kind !== 'source') continue
    if (!availableSourceIds.includes(entry.id)) availableSourceIds.push(entry.id)
    if (!Object.hasOwn(compositionState.document.sources, entry.id)) continue
    const source = compositionState.document.sources[entry.id]!
    const next: CompositionMedia = {
      id: entry.id,
      url: entry.url,
      filename: entry.filename,
      mediaType: source.kind,
      duration: source.durationTicks / COMPOSITION_TIME_BASE,
      width: source.width,
      height: source.height,
      fps: entry.fps ?? source.fps,
      vcodec: entry.vcodec ?? source.vcodec,
      acodec: entry.acodec ?? source.acodec,
    }
    if (JSON.stringify(media[entry.id]) !== JSON.stringify(next)) {
      media[entry.id] = next
      changed = true
    }
  }
  if (authoritative) {
    for (const id of Object.keys(media)) {
      if (Object.hasOwn(compositionState.document.sources, id) && !availableSourceIds.includes(id)) {
        delete media[id]
        changed = true
      }
    }
  }
  if (changed) {
    compositionState.media = media
    scheduleAutosave()
  }
}

/** Atomically replace a missing source id and its local media binding. */

export function relinkCompositionSource(
  sourceId: string,
  replacement: MediaEntry | CompositionSource,
): void {
  let entry: MediaEntry | null = null
  let replacementSource: CompositionSource
  if (isMediaEntry(replacement)) {
    if (replacement.kind !== 'source') throw new Error('Для relink нужен исходный файл из медиатеки')
    entry = replacement
    replacementSource = compositionSourceFromLibraryEntry(replacement)
  } else {
    replacementSource = { ...replacement }
  }
  const document = relinkCompositionDocumentSource(
    compositionState.document,
    sourceId,
    replacementSource,
  )

  const media = cloneCompositionMedia(compositionState.media)
  const oldMedia = media[sourceId]
  const registeredMedia = media[replacementSource.id]
  const url = entry?.url ?? registeredMedia?.url ?? oldMedia?.url
  const filename = entry?.filename ?? registeredMedia?.filename ?? oldMedia?.filename
  if (sourceId !== replacementSource.id) delete media[sourceId]
  if (url && filename) {
    media[replacementSource.id] = {
      id: replacementSource.id,
      url,
      filename,
      mediaType: replacementSource.kind,
      duration: replacementSource.durationTicks / COMPOSITION_TIME_BASE,
      width: replacementSource.width,
      height: replacementSource.height,
      fps: replacementSource.fps === undefined ? registeredMedia?.fps ?? oldMedia?.fps : replacementSource.fps,
      vcodec: replacementSource.vcodec === undefined ? registeredMedia?.vcodec ?? oldMedia?.vcodec : replacementSource.vcodec,
      acodec: replacementSource.acodec === undefined ? registeredMedia?.acodec ?? oldMedia?.acodec : replacementSource.acodec,
    }
  }
  commitDocument(document, media)
}


/** Stage source registration and clip replacement without changing the live editor. */
function prepareClipLibraryReplacement(clipId: string, entry: MediaEntry, processed = false) {
  if (entry.kind !== 'source') throw new Error('Выберите исходный файл из медиатеки')
  if (!entry.url || !entry.filename) throw new Error('У файла нет доступной привязки')
  const source = compositionSourceFromLibraryEntry(entry)
  const existing = compositionState.document.sources[source.id]
  if (existing && (existing.kind !== source.kind || existing.durationTicks !== source.durationTicks || existing.width !== source.width || existing.height !== source.height || existing.hasAudio !== source.hasAudio)) throw new Error('Метаданные файла отличаются от зарегистрированного источника')
  const staged = existing ? compositionState.document : registerSource(compositionState.document, source)
  return { document: replaceClipMedia(staged, clipId, source.id, undefined, processed), source }
}

export function replaceClipLibraryMediaReason(clipId: string, entry: MediaEntry): string | null {
  try { prepareClipLibraryReplacement(clipId, entry); return null }
  catch (error) { return error instanceof Error ? error.message : 'Не удалось заменить медиа' }
}

export function replaceCompositionClipFromLibrary(clipId: string, entry: MediaEntry, processed = false): void {
  const { document, source } = prepareClipLibraryReplacement(clipId, entry, processed)
  const binding: CompositionMedia = {
    id: source.id, url: entry.url, filename: entry.filename, mediaType: source.kind,
    duration: source.durationTicks / COMPOSITION_TIME_BASE, width: source.width, height: source.height,
    fps: source.fps, vcodec: source.vcodec, acodec: source.acodec,
  }
  commitDocument(document, { ...compositionState.media, [source.id]: binding })
}

export function compositionSourceFromMediaInfo(media: MediaInfo, kind = inferMediaType(media)): CompositionSource {
  const durationTicks = kind === 'image' ? 0 : Math.round(positiveNumber(media.duration) * COMPOSITION_TIME_BASE)
  if (kind !== 'image' && durationTicks <= 0) throw new Error('У файла нет корректной длительности')
  const width = kind === 'audio' ? 0 : positiveInteger(media.width)
  const height = kind === 'audio' ? 0 : positiveInteger(media.height)
  if (kind !== 'audio' && (!width || !height)) throw new Error('У файла нет корректных размеров')
  return {
    id: media.id,
    kind,
    durationTicks,
    width,
    height,
    hasAudio: kind === 'audio' || (kind === 'video' && media.acodec != null),
    fps: media.fps ?? null,
    vcodec: media.vcodec ?? null,
    acodec: media.acodec ?? null,
  }
}


export function compositionSourceFromLibraryEntry(entry: MediaEntry): CompositionSource {
  const kind = inferMediaType(entry)
  return compositionSourceFromMediaInfo({
    id: entry.id,
    url: entry.url,
    filename: entry.filename,
    mediaType: kind,
    duration: kind === 'image' ? 0 : positiveNumber(entry.duration),
    width: kind === 'audio' ? 0 : positiveInteger(entry.width),
    height: kind === 'audio' ? 0 : positiveInteger(entry.height),
    title: entry.title,
    fps: entry.fps,
    vcodec: entry.vcodec,
    acodec: entry.acodec,
  }, kind)
}


export function inferMediaType(media: {
  mediaType?: MediaType | null
  filename: string
  width?: number | null
  height?: number | null
}): MediaType {
  if (media.mediaType === 'video' || media.mediaType === 'audio' || media.mediaType === 'image') {
    return media.mediaType
  }
  const extension = media.filename.split('.').pop()?.toLowerCase()
  if (extension && ['png', 'jpg', 'jpeg', 'webp'].includes(extension)) return 'image'
  if (extension && ['aac', 'flac', 'm4a', 'mp3', 'ogg', 'opus', 'wav'].includes(extension)) return 'audio'
  return (media.width ?? 0) > 0 && (media.height ?? 0) > 0 ? 'video' : 'audio'
}


/** One authoring commit for the learned video and its linked source-audio group. */
export function replaceCompositionEnhancedVideoFromLibrary(clipId:string,entry:MediaEntry):void{
 if(entry.kind!=='source'||!entry.url||!entry.filename)throw new Error('Выберите исходное видео из медиатеки')
 const source=compositionSourceFromLibraryEntry(entry)
 const document=replaceEnhancedVideo(compositionState.document,clipId,source)
 const binding:CompositionMedia={id:source.id,url:entry.url,filename:entry.filename,mediaType:source.kind,duration:source.durationTicks/COMPOSITION_TIME_BASE,width:source.width,height:source.height,fps:source.fps,vcodec:source.vcodec,acodec:source.acodec}
 commitDocument(document,{...compositionState.media,[source.id]:binding})
}
export function setCompositionEnhancedVideoVersion(clipId:string,copy:boolean):void{
 commitDocument(switchEnhancedVideo(compositionState.document,clipId,copy))
}

/** Retain all editable cutout sources and the recipe in one authoring transaction. */
export function replaceCompositionCutoutFromLibrary(clipId:string,entry:import('$lib/api').CutoutAcceptedMedia):void{
 if(!entry.cutout){replaceCompositionClipFromLibrary(clipId,entry,true);return}
 const copy=compositionSourceFromLibraryEntry(entry),source=compositionSourceFromLibraryEntry(entry.cutout.source),mask=compositionSourceFromLibraryEntry(entry.cutout.mask)
 const document=replaceCutoutMedia(compositionState.document,clipId,{copy,source,mask,regions:entry.cutout.regions?.map(compositionSourceFromLibraryEntry)},entry.cutout.snapshot)
 const bindings={...compositionState.media}
 for(const data of [entry,entry.cutout.source,entry.cutout.mask,...(entry.cutout.regions??[])]){const value=compositionSourceFromLibraryEntry(data);bindings[value.id]={id:value.id,url:data.url,filename:data.filename,mediaType:value.kind,duration:value.durationTicks/COMPOSITION_TIME_BASE,width:value.width,height:value.height,fps:value.fps,vcodec:value.vcodec,acodec:value.acodec}}
 commitDocument(document,bindings)
}

export function replaceCompositionVideoCutoutFromLibrary(clipId:string,entry:import('$lib/api').SavedPersonVideo):void{
 if(!entry.videoCutout){replaceCompositionEnhancedVideoFromLibrary(clipId,entry);return}
 const copy=compositionSourceFromLibraryEntry(entry),source=compositionSourceFromLibraryEntry(entry.videoCutout.source),mask=compositionSourceFromLibraryEntry(entry.videoCutout.mask)
 const document=replaceVideoCutoutMedia(compositionState.document,clipId,{copy,source,mask},entry.videoCutout.snapshot),bindings={...compositionState.media}
 for(const data of [entry,entry.videoCutout.source,entry.videoCutout.mask]){const value=compositionSourceFromLibraryEntry(data);bindings[value.id]={id:value.id,url:data.url,filename:data.filename,mediaType:value.kind,duration:value.durationTicks/COMPOSITION_TIME_BASE,width:value.width,height:value.height,fps:value.fps,vcodec:value.vcodec,acodec:value.acodec}}
 commitDocument(document,bindings)
}


export function attachCompositionVideoRegionFromLibrary(clipId:string,expected:import('$lib/composition/types').VideoMaskAttachment,entry:import('$lib/api').SavedSamVideoRegion):void{
 const document=attachCompositionVideoRegion(compositionState.document,clipId,expected,compositionSourceFromLibraryEntry(entry.source),compositionSourceFromLibraryEntry(entry.mask),entry.snapshot)
 if(document===compositionState.document)return
 const bindings={...compositionState.media}
 for(const data of [entry.source,entry.mask]){const value=compositionSourceFromLibraryEntry(data);bindings[value.id]={id:value.id,url:data.url,filename:data.filename,mediaType:value.kind,duration:value.durationTicks/COMPOSITION_TIME_BASE,width:value.width,height:value.height,fps:value.fps,vcodec:value.vcodec,acodec:value.acodec}}
 commitDocument(document,bindings)
}
