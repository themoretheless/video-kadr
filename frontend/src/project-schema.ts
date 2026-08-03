export const PROJECT_DOCUMENT_SCHEMA_VERSION = 3 as const
export const PROJECT_ENVELOPE_SCHEMA_VERSION = 1 as const
export const PROJECT_TIME_BASE = 1_000_000

export type JsonObject = Record<string, unknown>

export interface ProjectDocument extends JsonObject {
  schemaVersion: typeof PROJECT_DOCUMENT_SCHEMA_VERSION
  name: string
  primaryMediaId: string
  activeSequenceId: string
  media: ProjectMedia[]
  sequences: ProjectSequence[]
  legacyFields?: JsonObject
  proxyPolicy?: 'auto' | 'original' | 'proxy'
}

export interface ProjectMedia extends JsonObject {
  id: string
  kind: string
  assetRef?: string
  contentFingerprint?: string
  metadata: JsonObject
}

export interface ProjectSequence extends JsonObject {
  id: string
  name: string
  settings: SequenceSettings
  tracks: ProjectTrack[]
}

export interface SequenceSettings extends JsonObject {
  timeBase: number
  frameRate?: number
  width?: number
  height?: number
}

export interface ProjectTrack extends JsonObject {
  id: string
  kind: string
  name: string
  clips: ProjectClip[]
  muted?: boolean
  solo?: boolean
  locked?: boolean
  hidden?: boolean
}

export interface ProjectClip extends JsonObject {
  id: string
  mediaId: string
  timelineStartTick: number
  durationTicks: number
  sourceInTick: number
  sourceOutTick: number
  effects: ProjectEffect[]
}

export interface ProjectEffect extends JsonObject {
  id: string
  kind: string
  enabled: boolean
  parameters: JsonObject
}

export interface ProjectEnvelope extends JsonObject {
  schemaVersion: typeof PROJECT_ENVELOPE_SCHEMA_VERSION
  projectId: string
  revision: number
  createdAt: number
  updatedAt: number
  document: ProjectDocument
}

export interface LegacyProjectPayload extends JsonObject {
  schemaVersion?: 1
  videoId: string
  name?: string
  video: JsonObject
  edit: JsonObject
}

export function migrateProjectDocument(value: unknown): ProjectDocument {
  const object = asObject(value, 'project document')
  const rawVersion = object.schemaVersion ?? 1
  if (!Number.isInteger(rawVersion) || typeof rawVersion !== 'number') {
    throw new Error('invalid project schemaVersion')
  }
  if (rawVersion === 1) return migrateV1(object)
  if (rawVersion === 2) return migrateV2(object)
  if (rawVersion !== PROJECT_DOCUMENT_SCHEMA_VERSION) {
    throw new Error(
      `unsupported project schemaVersion ${rawVersion}; latest supported is ${PROJECT_DOCUMENT_SCHEMA_VERSION}`,
    )
  }
  const document = cloneJson(object) as ProjectDocument
  document.proxyPolicy ??= 'auto'
  for (const media of document.media ?? []) {
    if (media && typeof media === 'object' && media.metadata) {
      media.metadata = durableMediaMetadata(media.metadata)
    }
  }
  validateProjectDocument(document)
  return document
}

export function createProjectDocumentFromLegacy(
  videoId: string,
  name: string,
  video: JsonObject,
  edit: JsonObject,
): ProjectDocument {
  return migrateProjectDocument({ schemaVersion: 1, videoId, name, video, edit })
}

export function validateProjectDocument(document: ProjectDocument): void {
  if (document.schemaVersion !== PROJECT_DOCUMENT_SCHEMA_VERSION) {
    throw new Error(`unsupported project schemaVersion ${String(document.schemaVersion)}`)
  }
  validateId(document.primaryMediaId, 'primaryMediaId')
  validateId(document.activeSequenceId, 'activeSequenceId')
  if (!document.name.trim()) throw new Error('invalid project name')
  if (document.legacyFields !== undefined) asObject(document.legacyFields, 'legacyFields')
  if (document.proxyPolicy !== undefined && !['auto', 'original', 'proxy'].includes(document.proxyPolicy)) throw new Error('invalid project proxyPolicy')
  if (!Array.isArray(document.media) || !Array.isArray(document.sequences)) {
    throw new Error('invalid project collections')
  }

  const mediaIds = new Set<string>()
  const mediaById = new Map<string, ProjectMedia>()
  for (const media of document.media) {
    validateId(media.id, 'media.id')
    validateToken(media.kind, 'media.kind')
    if (media.assetRef !== undefined) validateId(media.assetRef, 'media.assetRef')
    if (
      media.contentFingerprint !== undefined &&
      !/^[a-f0-9]{64}$/.test(media.contentFingerprint)
    ) throw new Error('invalid media.contentFingerprint')
    asObject(media.metadata, 'media.metadata')
    addUnique(mediaIds, media.id)
    mediaById.set(media.id, media)
  }
  if (!mediaIds.has(document.primaryMediaId)) {
    throw new Error(`missing project reference ${document.primaryMediaId}`)
  }

  const sequenceIds = new Set<string>()
  const trackIds = new Set<string>()
  const clipIds = new Set<string>()
  const effectIds = new Set<string>()
  for (const sequence of document.sequences) {
    validateId(sequence.id, 'sequence.id')
    addUnique(sequenceIds, sequence.id)
    if (!sequence.name.trim() || !isU32(sequence.settings?.timeBase)) {
      throw new Error('invalid project sequence')
    }
    if (
      sequence.settings.frameRate !== undefined &&
      (!Number.isFinite(sequence.settings.frameRate) || sequence.settings.frameRate <= 0)
    ) {
      throw new Error('invalid sequence frameRate')
    }
    if (
      (sequence.settings.width !== undefined && !isU32(sequence.settings.width)) ||
      (sequence.settings.height !== undefined && !isU32(sequence.settings.height))
    ) {
      throw new Error('invalid sequence dimensions')
    }
    if (!Array.isArray(sequence.tracks)) throw new Error('invalid project tracks')
    for (const track of sequence.tracks) {
      validateId(track.id, 'track.id')
      validateToken(track.kind, 'track.kind')
      if (typeof track.name !== 'string') throw new Error('invalid track.name')
      for (const key of ['muted', 'solo', 'locked', 'hidden'] as const) {
        if (track[key] !== undefined && typeof track[key] !== 'boolean') {
          throw new Error(`invalid track.${key}`)
        }
      }
      addUnique(trackIds, track.id)
      if (!Array.isArray(track.clips)) throw new Error('invalid project clips')
      for (const clip of track.clips) {
        validateId(clip.id, 'clip.id')
        addUnique(clipIds, clip.id)
        const media = mediaById.get(clip.mediaId)
        if (!media) throw new Error(`missing project reference ${clip.mediaId}`)
        const compatible =
          (track.kind === 'video' && (media.kind === 'video' || media.kind === 'image')) ||
          (track.kind === 'audio' && media.kind === 'audio')
        if (!compatible) throw new Error('incompatible project media and track')
        if (
          !isNonNegativeInteger(clip.timelineStartTick) ||
          !isPositiveInteger(clip.durationTicks) ||
          !isNonNegativeInteger(clip.sourceInTick) ||
          !isPositiveInteger(clip.sourceOutTick) ||
          clip.sourceOutTick <= clip.sourceInTick ||
          clip.sourceOutTick - clip.sourceInTick !== clip.durationTicks ||
          clip.timelineStartTick + clip.durationTicks > Number.MAX_SAFE_INTEGER
        ) {
          throw new Error('invalid project clip range')
        }
        const sourceDuration = sourceDurationTicks(media.metadata, sequence.settings.timeBase)
        if (sourceDuration !== undefined && clip.sourceOutTick > sourceDuration) {
          throw new Error('invalid project clip source range')
        }
        if (!Array.isArray(clip.effects)) throw new Error('invalid project effects')
        for (const effect of clip.effects) {
          validateId(effect.id, 'effect.id')
          validateToken(effect.kind, 'effect.kind')
          addUnique(effectIds, effect.id)
          if (typeof effect.enabled !== 'boolean') throw new Error('invalid effect enabled')
          asObject(effect.parameters, 'effect.parameters')
        }
      }
      const orderedClips = [...track.clips].sort(
        (left, right) => left.timelineStartTick - right.timelineStartTick,
      )
      for (let index = 1; index < orderedClips.length; index++) {
        const previous = orderedClips[index - 1]!
        const current = orderedClips[index]!
        if (previous.timelineStartTick + previous.durationTicks > current.timelineStartTick) {
          throw new Error('overlapping project clips')
        }
      }
    }
  }
  if (!sequenceIds.has(document.activeSequenceId)) {
    throw new Error(`missing project reference ${document.activeSequenceId}`)
  }
}

function sourceDurationTicks(metadata: JsonObject, timeBase: number): number | undefined {
  const duration = metadata.duration
  if (typeof duration !== 'number' || !Number.isFinite(duration) || duration <= 0) return undefined
  const ticks = Math.round(duration * timeBase)
  return Number.isSafeInteger(ticks) ? ticks : undefined
}

export function legacyProjectValues(document: ProjectDocument): {
  video: JsonObject
  edit: JsonObject
} {
  const video =
    document.media.find((media) => media.id === document.primaryMediaId)?.metadata ?? {}
  const sequence = document.sequences.find((item) => item.id === document.activeSequenceId)
  const effect = sequence?.tracks
    .flatMap((track) => track.clips)
    .filter((clip) => clip.mediaId === document.primaryMediaId)
    .flatMap((clip) => clip.effects)
    .find((item) => item.kind === 'legacy_edit')
  return { video, edit: effect?.parameters ?? {} }
}

/** Update compatibility fields without rebuilding and losing native tracks/extensions. */
export function updateLegacyProjectValues(
  document: ProjectDocument,
  name: string,
  video: JsonObject,
  edit: JsonObject,
): ProjectDocument {
  const next = cloneJson(document)
  next.name = name
  const media = next.media.find((item) => item.id === next.primaryMediaId)
  if (!media) throw new Error(`missing project reference ${next.primaryMediaId}`)
  media.assetRef ??= stringValue(video.assetId) ?? media.id
  const fingerprint = fingerprintValue(video.fingerprint)
  if (!media.contentFingerprint && fingerprint) media.contentFingerprint = fingerprint
  media.metadata = withoutAssetIdentity({ ...media.metadata, ...video })
  const sequence = next.sequences.find((item) => item.id === next.activeSequenceId)
  let effect = sequence?.tracks
    .flatMap((track) => track.clips)
    .filter((clip) => clip.mediaId === next.primaryMediaId)
    .flatMap((clip) => clip.effects)
    .find((item) => item.kind === 'legacy_edit')
  if (!effect) {
    const clip = sequence?.tracks.flatMap((track) => track.clips).find((item) => item.mediaId === next.primaryMediaId)
    if (clip) {
      const usedIds = new Set(next.sequences.flatMap((item) => item.tracks).flatMap((track) => track.clips).flatMap((item) => item.effects).map((item) => item.id))
      let suffix = 0
      let effectId = 'effect-legacy-edit'
      while (usedIds.has(effectId)) effectId = `effect-legacy-edit-${++suffix}`
      effect = { id: effectId, kind: 'legacy_edit', enabled: true, parameters: {} }
      clip.effects.push(effect)
    }
  }
  if (effect) effect.parameters = { ...effect.parameters, ...edit }
  validateProjectDocument(next)
  return next
}

export function ensureCreatorTrackLayout(
  document: ProjectDocument,
  sequenceId = document.activeSequenceId,
): ProjectDocument {
  const next = cloneJson(document)
  const sequence = next.sequences.find((item) => item.id === sequenceId)
  if (!sequence) throw new Error(`missing project reference ${sequenceId}`)
  const usedIds = new Set(next.sequences.flatMap((item) => item.tracks).map((track) => track.id))
  for (const [kind, label] of [
    ['video', 'Видео'],
    ['audio', 'Аудио'],
  ] as const) {
    let count = sequence.tracks.filter((track) => track.kind === kind).length
    while (count < 4) {
      const ordinal = count + 1
      let suffix = ordinal
      let id = `track-${kind}-${suffix}`
      while (usedIds.has(id)) id = `track-${kind}-${++suffix}`
      usedIds.add(id)
      sequence.tracks.push({
        id,
        kind,
        name: `${label} ${ordinal}`,
        clips: [],
        muted: false,
        solo: false,
        locked: false,
        hidden: false,
      })
      count++
    }
  }
  validateProjectDocument(next)
  return next
}

export function createProjectEnvelope(
  projectId: string,
  document: ProjectDocument,
  timestamp: number,
): ProjectEnvelope {
  const envelope: ProjectEnvelope = {
    schemaVersion: PROJECT_ENVELOPE_SCHEMA_VERSION,
    projectId,
    revision: 1,
    createdAt: timestamp,
    updatedAt: timestamp,
    document,
  }
  validateProjectEnvelope(envelope)
  return envelope
}

export function decodeProjectEnvelope(value: unknown): ProjectEnvelope {
  const envelope = asObject(value, 'project envelope')
  if (envelope.schemaVersion !== PROJECT_ENVELOPE_SCHEMA_VERSION) {
    throw new Error(
      `unsupported project envelope schemaVersion ${String(envelope.schemaVersion)}; expected ${PROJECT_ENVELOPE_SCHEMA_VERSION}`,
    )
  }
  const decoded = envelope as ProjectEnvelope
  decoded.document = migrateProjectDocument(decoded.document)
  validateProjectEnvelope(decoded)
  return decoded
}

export function nextProjectRevision(
  envelope: ProjectEnvelope,
  document: ProjectDocument,
  updatedAt: number,
): ProjectEnvelope {
  if (envelope.revision >= Number.MAX_SAFE_INTEGER) throw new Error('project revision overflow')
  const next: ProjectEnvelope = {
    ...envelope,
    schemaVersion: PROJECT_ENVELOPE_SCHEMA_VERSION,
    projectId: envelope.projectId,
    revision: envelope.revision + 1,
    createdAt: envelope.createdAt,
    updatedAt,
    document,
  }
  validateProjectEnvelope(next)
  return next
}

function validateProjectEnvelope(envelope: ProjectEnvelope): void {
  validateId(envelope.projectId, 'projectId')
  if (!isPositiveInteger(envelope.revision)) throw new Error('invalid project revision')
  if (
    !isNonNegativeInteger(envelope.createdAt) ||
    !isNonNegativeInteger(envelope.updatedAt) ||
    envelope.updatedAt < envelope.createdAt
  ) {
    throw new Error('invalid project timestamps')
  }
  validateProjectDocument(envelope.document)
}

function migrateV1(value: JsonObject): ProjectDocument {
  const video = durableMediaMetadata(asObject(value.video, 'video'))
  const edit = asObject(value.edit, 'edit')
  const videoId = stringValue(value.videoId) ?? stringValue(video.id)
  if (!videoId) throw new Error('missing project videoId')
  const name =
    stringValue(value.name) ??
    stringValue(video.title) ??
    stringValue(video.filename) ??
    'Без названия'
  const durationTicks = durationToTicks(video.duration)
  const settings: SequenceSettings = { timeBase: PROJECT_TIME_BASE }
  const frameRate = positiveNumber(video.fps)
  const width = positiveInteger(video.width)
  const height = positiveInteger(video.height)
  if (frameRate !== undefined) settings.frameRate = frameRate
  if (width !== undefined) settings.width = width
  if (height !== undefined) settings.height = height
  const explicitKind = stringValue(video.mediaKind)
  const primaryKind = explicitKind === 'audio' || explicitKind === 'video'
    ? explicitKind
    : stringValue(video.acodec) && !stringValue(video.vcodec) && width === undefined
      ? 'audio'
      : 'video'
  const primaryClip: ProjectClip = {
    id: 'clip-main',
    mediaId: videoId,
    timelineStartTick: 0,
    durationTicks,
    sourceInTick: 0,
    sourceOutTick: durationTicks,
    effects: [
      {
        id: 'effect-legacy-edit',
        kind: 'legacy_edit',
        enabled: true,
        parameters: edit,
      },
    ],
  }

  const known = new Set(['schemaVersion', 'videoId', 'name', 'video', 'edit'])
  const legacyFields = Object.fromEntries(
    Object.entries(value).filter(([key]) => !known.has(key)),
  )
  const document: ProjectDocument = {
    schemaVersion: PROJECT_DOCUMENT_SCHEMA_VERSION,
    proxyPolicy: 'auto',
    name,
    primaryMediaId: videoId,
    activeSequenceId: 'sequence-main',
    media: [{
      id: videoId,
      kind: primaryKind,
      assetRef: stringValue(video.assetId) ?? videoId,
      ...(fingerprintValue(video.fingerprint) ? { contentFingerprint: fingerprintValue(video.fingerprint) } : {}),
      metadata: withoutAssetIdentity(video),
    }],
    sequences: [
      {
        id: 'sequence-main',
        name: 'Основная',
        settings,
        tracks: [
          {
            id: 'track-video-main',
            kind: 'video',
            name: 'Видео 1',
            clips: primaryKind === 'video' ? [primaryClip] : [],
          },
          {
            id: 'track-audio-main',
            kind: 'audio',
            name: 'Аудио 1',
            clips: primaryKind === 'audio' ? [primaryClip] : [],
          },
        ],
      },
    ],
  }
  if (Object.keys(legacyFields).length) document.legacyFields = legacyFields
  validateProjectDocument(document)
  return document
}

function migrateV2(value: JsonObject): ProjectDocument {
  const document = cloneJson(value) as unknown as ProjectDocument
  document.schemaVersion = PROJECT_DOCUMENT_SCHEMA_VERSION
  document.proxyPolicy ??= 'auto'
  for (const media of document.media ?? []) {
    const metadata = durableMediaMetadata(asObject(media.metadata, 'media.metadata'))
    media.assetRef = stringValue(media.assetRef) ?? stringValue(metadata.assetId) ?? media.id
    const fingerprint = fingerprintValue(media.contentFingerprint) ?? fingerprintValue(metadata.fingerprint)
    if (fingerprint) media.contentFingerprint = fingerprint
    else delete media.contentFingerprint
    media.metadata = withoutAssetIdentity(metadata)
  }
  validateProjectDocument(document)
  return document
}

function asObject(value: unknown, field: string): JsonObject {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`invalid ${field}`)
  }
  return value as JsonObject
}

function validateId(value: unknown, field: string): asserts value is string {
  if (typeof value !== 'string' || !value.trim() || value.length > 128) {
    throw new Error(`invalid ${field}`)
  }
}

function validateToken(value: unknown, field: string): asserts value is string {
  if (typeof value !== 'string' || !value.trim() || value.length > 64) {
    throw new Error(`invalid ${field}`)
  }
}

function addUnique(values: Set<string>, value: string): void {
  if (values.has(value)) throw new Error(`duplicate project id ${value}`)
  values.add(value)
}

function stringValue(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined
}

function fingerprintValue(value: unknown): string | undefined {
  return typeof value === 'string' && /^[a-f0-9]{64}$/.test(value) ? value : undefined
}

function withoutAssetIdentity(metadata: JsonObject): JsonObject {
  const durable = durableMediaMetadata(metadata)
  delete durable.assetId
  delete durable.fingerprint
  return durable
}

function positiveNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : undefined
}

function positiveInteger(value: unknown): number | undefined {
  return isPositiveInteger(value) ? value : undefined
}

function isPositiveInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0
}

function isU32(value: unknown): value is number {
  return isPositiveInteger(value) && value <= 0xffff_ffff
}

function durationToTicks(value: unknown): number {
  const duration = positiveNumber(value)
  if (duration === undefined) return 1
  const ticks = Math.round(duration * PROJECT_TIME_BASE)
  return isPositiveInteger(ticks) ? ticks : 1
}

function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function durableMediaMetadata(metadata: JsonObject): JsonObject {
  const durable = cloneJson(metadata)
  delete durable.url
  delete durable.path
  delete durable.file
  delete durable.availability
  return durable
}
