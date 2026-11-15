import type { NativeNestedProject } from '$lib/api'
import type { WireTextClip } from './types'

export const CONCAT_CAPTION_CLIPS = 'video-kadr-caption-clips.json'
export const MAX_CONCAT_CAPTION_BYTES = 8 * 1024 * 1024
type ImportedAnimation = { offsetX: number; offsetY: number }
type Captions = readonly (readonly [timelineId: string, clip: WireTextClip, importedAnimation?: ImportedAnimation])[]

function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined
}

function sameJson(left: unknown, right: unknown): boolean {
  if (left === right) return true
  if (Array.isArray(left)) {
    return Array.isArray(right) && left.length === right.length && left.every((value, index) => sameJson(value, right[index]))
  }
  const a = record(left)
  const b = record(right)
  return !!a && !!b && Object.keys(a).length === Object.keys(b).length &&
    Object.keys(a).every(key => Object.hasOwn(b, key) && sameJson(a[key], b[key]))
}

// Concat's writer adds some defaults and omits others when saving a title.
function normalizedClip(value: unknown): unknown {
  const clip = record(value)
  if (!clip) return value
  return {
    mediaId: '', sourceStart: 0, opacity: 1, volume: 1, fadeIn: 0, fadeOut: 0, speed: 1,
    preservePitch: true, stretchX: 1, stretchY: 1, flipH: false, flipV: false, keys: [], filters: [], videoEffects: [],
    ...clip,
    blend: clip.blend || 'normal',
  }
}

function normalizedVideo(value: unknown): unknown {
  const video = record(value)
  return video ? { colorSpace: 'sdr', ...video } : value
}

function hasStaticCaptionPose(clip: WireTextClip): boolean {
  const transform = clip.transform
  return !!transform && !transform.position &&
    (!transform.concatAnimation || Object.keys(transform.concatAnimation.keys).every(captionKey)) &&
    !clip.colorAnimation && !clip.effects?.length &&
    [transform.x, transform.y, transform.scaleX, transform.scaleY, transform.rotationDegrees]
      .every(value => value?.mode === 'constant')
}

function captionKey(key: unknown): boolean {
  return key === 'opacity' || key === 'offsetX' || key === 'offsetY'
}

function supportedCaptionKeys(value: unknown): boolean {
  return value === undefined || Array.isArray(value) && value.every(key => captionKey(record(key)?.property))
}

function mergeCaptionEdits(clip: WireTextClip, baseline: unknown, current: unknown): readonly [WireTextClip, ImportedAnimation?] | undefined {
  if (sameJson(normalizedClip(baseline), normalizedClip(current))) return [clip]
  const before = record(baseline)
  const after = record(current)
  const previousText = record(before?.text)
  const editedText = record(after?.text)
  if (!before || !after || before.kind !== 'text' || !previousText || !editedText ||
      typeof editedText.content !== 'string' || typeof after.start !== 'number' ||
      !Number.isFinite(after.start) || after.start < 0 || after.start > 86400 ||
      typeof after.duration !== 'number' || !Number.isFinite(after.duration) ||
      after.duration <= 0 || after.duration > 86400) return undefined
  const duration = Math.round(after.duration * 1_000_000)
  const resized = duration !== clip.timelineEndTick - clip.timelineStartTick
  if (duration < 1 || (resized && !hasStaticCaptionPose(clip))) return undefined
  const retainKeys = resized && (clip.opacity?.mode === 'keyframes' || !!clip.transform?.concatAnimation ||
    Array.isArray(after.keys) && after.keys.length > 0)
  if (resized && !retainKeys && clip.opacity?.mode !== 'constant') return undefined
  if (retainKeys && (!supportedCaptionKeys(before.keys) || !supportedCaptionKeys(after.keys))) return undefined
  if (retainKeys && clip.opacity?.mode === 'keyframes' && (!Array.isArray(before.keys) || !before.keys.length)) return undefined
  const offsetX = typeof before.offsetX === 'number' ? before.offsetX : 0
  const offsetY = typeof before.offsetY === 'number' ? before.offsetY : 0


  // Text and whole-clip moves retain the original local clock. A static title
  // also accepts Concat's new duration and, for a head trim, its source in-point.
  // The imported document already carries the updated display name in clipNames.
  const unchangedFields = {
    ...after,
    start: before.start,
    name: before.name,
    ...(retainKeys ? { keys: before.keys ?? [], opacity: before.opacity ?? 1 } : {}),
    ...(resized ? { duration: before.duration, sourceStart: before.sourceStart ?? 0 } : {}),
    text: { ...editedText, content: previousText.content },
  }
  const comparison = { ...before, name: before.name }
  if (!sameJson(normalizedClip(comparison), normalizedClip(unchangedFields))) return undefined
  const start = Math.round(after.start * 1_000_000)
  const restored = {
    ...clip,
    text: editedText.content,
    timelineStartTick: start,
    timelineEndTick: start + duration,
  }
  return retainKeys ? [restored, { offsetX, offsetY }] : [restored]
}

export async function readConcatTemplateCaptions(file: File, manifest: File): Promise<Captions | undefined> {
  if (file.size > MAX_CONCAT_CAPTION_BYTES) throw new Error('Дорожки субтитров шаблона превышают 8 МиБ')
  const sidecar: unknown = JSON.parse(await file.text())
  if (!sidecar || typeof sidecar !== 'object' || !('manifestSha256' in sidecar) ||
      typeof sidecar.manifestSha256 !== 'string' || !/^[0-9a-f]{64}$/.test(sidecar.manifestSha256) ||
      !('clips' in sidecar) || !Array.isArray(sidecar.clips)) {
    throw new Error('Некорректные дорожки субтитров шаблона')
  }
  const digest = await crypto.subtle.digest('SHA-256', await manifest.arrayBuffer())
  const hash = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('')
  const unchanged = hash === sidecar.manifestSha256
  const timelines = new Map<string, { video: unknown; clips: Map<string, unknown> }>()
  if (!unchanged) {
    const document = record(JSON.parse(await manifest.text()))
    for (const value of Array.isArray(document?.timelines) ? document.timelines : []) {
      const timeline = record(value)
      if (!timeline || typeof timeline.id !== 'string' || !Array.isArray(timeline.clips)) continue
      const clips = new Map<string, unknown>()
      for (const entry of timeline.clips) {
        const clip = record(entry)
        if (typeof clip?.id === 'string') clips.set(clip.id, clip)
      }
      timelines.set(timeline.id, { video: timeline.video, clips })
    }
  }
  const eligible: [string, WireTextClip, ImportedAnimation?][] = []
  for (const entry of sidecar.clips) {
    if (!Array.isArray(entry) || (entry.length !== 2 && entry.length !== 3) || typeof entry[0] !== 'string' ||
        !entry[1] || typeof entry[1] !== 'object' || typeof entry[1].id !== 'string') {
      throw new Error('Некорректный клип субтитров шаблона')
    }
    const [timelineId, clip, baselineValue] = entry
    const baseline = record(baselineValue)
    const timeline = timelines.get(timelineId)
    if (unchanged) {
      eligible.push([timelineId, clip])
    } else if (baseline && timeline && sameJson(normalizedVideo(baseline.video), normalizedVideo(timeline.video))) {
      const merged = mergeCaptionEdits(clip, baseline.clip, timeline.clips.get(clip.id))
      if (merged) eligible.push([timelineId, ...merged])
    }
  }
  return eligible.length ? eligible : undefined
}

export function restoreConcatTemplateCaptions(
  project: NativeNestedProject,
  timelineIds: Readonly<Record<string, string>>,
  captions: Captions | undefined,
): NativeNestedProject {
  if (!captions?.length) return project
  const groups = new Map<string, Map<string, { clip: WireTextClip; importedAnimation?: ImportedAnimation }>>()
  for (const [timeline, clip, importedAnimation] of captions) {
    const id = Object.hasOwn(timelineIds, timeline) ? timelineIds[timeline] : undefined
    if (!id || !Object.hasOwn(project.compositions, id)) throw new Error('Таймлайн субтитров отсутствует в шаблоне')
    const group = groups.get(id) ?? new Map<string, { clip: WireTextClip; importedAnimation?: ImportedAnimation }>()
    if (group.has(clip.id)) throw new Error('Повторный клип субтитров шаблона')
    group.set(clip.id, { clip, importedAnimation })
    groups.set(id, group)
  }
  const compositions = { ...project.compositions }
  for (const [id, group] of groups) {
    const node = compositions[id]!
    const roles = new Set(node.content.metadata?.captionTextTracks)
    const tracks = node.content.tracks.map(track => {
      if (track.kind !== 'text') return track
      const clips = track.clips.map(clip => {
        const entry = group.get(clip.id)
        if (!entry) return clip
        const { clip: restored, importedAnimation } = entry
        if (restored.text !== clip.text || restored.timelineStartTick !== clip.timelineStartTick ||
            restored.timelineEndTick !== clip.timelineEndTick) {
          throw new Error('Клип субтитров расходится с манифестом шаблона')
        }
        group.delete(clip.id)
        roles.add(track.id)
        if (!importedAnimation) return restored
        const animation = clip.transform.concatAnimation
        const { x, y } = restored.transform
        if (!animation) {
          return { ...restored, opacity: clip.opacity, transform: { ...restored.transform, concatAnimation: undefined } }
        }
        if (x.mode !== 'constant' || y.mode !== 'constant' ||
            !Object.keys(animation.keys).every(captionKey)) {
          throw new Error('Некорректная анимация субтитров')
        }
        const original = restored.transform.concatAnimation
        const dx = (original?.rest.x ?? x.value / animation.canvas[0]) - importedAnimation.offsetX
        const dy = (original?.rest.y ?? y.value / animation.canvas[1]) - importedAnimation.offsetY
        const keys = { ...animation.keys }
        if (keys.offsetX) keys.offsetX = keys.offsetX.map(key => ({ ...key, value: key.value + dx }))
        if (keys.offsetY) keys.offsetY = keys.offsetY.map(key => ({ ...key, value: key.value + dy }))
        return {
          ...restored, opacity: clip.opacity, transform: { ...restored.transform, concatAnimation: {
            ...animation, keys, rest: { ...animation.rest, x: animation.rest.x + dx, y: animation.rest.y + dy },
          } },
        }
      })
      return { ...track, clips }
    })
    if (group.size) throw new Error('Клип субтитров отсутствует в шаблоне')
    compositions[id] = { ...node, content: { ...node.content, tracks, metadata: {
      ...node.content.metadata, captionTextTracks: [...roles],
    } } }
  }
  return { ...project, compositions }
}
