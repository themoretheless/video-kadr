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
  rememberMedia,
  scheduleAutosave,
  selectCompositionClip,
  validFps
} from './core.svelte.js'

export function addMediaInfoToComposition(media: MediaInfo): string {
  const kind = inferMediaType(media)
  const source = compositionSourceFromMediaInfo(media, kind)
  let document = compositionState.document
  if (!Object.hasOwn(document.sources, source.id)) document = registerSource(document, source)
  rememberMedia(media, kind)

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
        },
      }
      assertValidComposition(document)
    }
    const result = addVideo(document, source)
    document = result.document
    clipId = result.clipId
  } else if (kind === 'audio') {
    const result = addAudio(document, source)
    document = result.document
    clipId = result.clipId
  } else {
    const result = addImage(document, source)
    document = result.document
    clipId = result.clipId
  }

  commitDocument(document)
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

