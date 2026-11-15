import type { NativeNestedProject } from '$lib/api'
import { MAX_COMPOSITION_TRACKS, MAX_COMPOSITION_VISIBLE_LAYERS, type WireCompositionTrack, type WireTextClip } from './types'
import { colorToRgba } from './payload'

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

const cropEdges = ['left', 'top', 'right', 'bottom']

function normalizedCrop(value: unknown): unknown {
  const crop = record(value)
  return crop && Object.entries(crop).every(([edge, amount]) => cropEdges.includes(edge) && amount === 0)
    ? null : value ?? null
}

// Concat's writer adds some defaults and omits others when saving a title.
function normalizedClip(value: unknown): unknown {
  const clip = record(value)
  if (!clip) return value
  return {
    mediaId: '', sourceStart: 0, scale: 1, rotation: 0, offsetX: 0, offsetY: 0, opacity: 1, volume: 1, fadeIn: 0, fadeOut: 0, speed: 1,
    preservePitch: true, stretchX: 1, stretchY: 1, flipH: false, flipV: false, keys: [], filters: [], videoEffects: [],
    ...clip,
    blend: clip.blend || 'normal',
    crop: normalizedCrop(clip.crop),
  }
}

function captionCanvas(video: Record<string, unknown>, shared?: Record<string, unknown>) {
  // Caption positions use canvas pixels and their local clock uses microseconds.
  return {
    width: video.width ?? shared?.width ?? 1920,
    height: video.height ?? shared?.height ?? 1080,
    colorSpace: video.colorSpace ?? 'sdr',
  }
}

function hasStaticCaptionPose(clip: WireTextClip): boolean {
  const transform = clip.transform
  return !!transform && !transform.position &&
    (!transform.concatAnimation || Object.keys(transform.concatAnimation.keys).every(key => captionKey(key, centeredCaption(clip)))) &&
    !clip.colorAnimation && !clip.effects?.length &&
    [transform.x, transform.y, transform.scaleX, transform.scaleY, transform.rotationDegrees]
      .every(value => value?.mode === 'constant')
}

function centeredCaption(clip: WireTextClip): boolean {
  const transform = clip.transform
  const anchor = clip.textAnchor ?? [transform?.anchorX, transform?.anchorY]
  return clip.style?.align === 'center' && transform?.anchorX === 0.5 && transform.anchorY === 0.5 &&
    anchor[0] === 0.5 && anchor[1] === 0.5
}

function captionKey(key: unknown, centered: boolean): boolean {
  return key === 'opacity' || key === 'offsetX' || key === 'offsetY' ||
    centered && (key === 'scale' || key === 'rotation')
}

function supportedCaptionKeys(value: unknown, centered: boolean): boolean {
  return value === undefined || Array.isArray(value) && value.every(key => captionKey(record(key)?.property, centered))
}

function mergeCaptionEdits(clip: WireTextClip, baseline: unknown, current: unknown, video: unknown): readonly [WireTextClip, ImportedAnimation?] | undefined {
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
  const editedKeys = !sameJson(before.keys ?? [], after.keys ?? [])
  if (duration < 1 || ((resized || editedKeys) && !hasStaticCaptionPose(clip))) return undefined
  const retainKeys = editedKeys || (resized && (clip.opacity?.mode === 'keyframes' || !!clip.transform?.concatAnimation ||
    Array.isArray(after.keys) && after.keys.length > 0))
  if (resized && !retainKeys && clip.opacity?.mode !== 'constant') return undefined
  const centered = centeredCaption(clip)
  if (retainKeys && (!supportedCaptionKeys(before.keys, centered) || !supportedCaptionKeys(after.keys, centered))) return undefined
  if (retainKeys && clip.opacity?.mode === 'keyframes' && (!Array.isArray(before.keys) || !before.keys.length)) return undefined

  let style = clip.style
  const comparisonText: Record<string, unknown> = { ...editedText, content: previousText.content }
  const retainOriginalField = (key: string) => {
    if (Object.hasOwn(previousText, key)) comparisonText[key] = previousText[key]
    else delete comparisonText[key]
  }
  for (const [source, target, fallback] of [
    ['color', 'color', '#ffffff'], ['strokeColor', 'stroke', '#000000'], ['background', 'background', ''],
  ] as const) {
    if (editedText[source] === previousText[source]) continue
    const color = editedText[source] === undefined ? fallback : editedText[source]
    if (!style || clip.concatTitle || (target === 'color' && clip.colorAnimation) ||
        typeof color !== 'string' || !(/^#(?:[a-f0-9]{6}|[a-f0-9]{8})$/i.test(color) ||
          (target === 'background' && color === ''))) return undefined
    style = { ...style, [target]: colorToRgba(color || '#00000000') }
    retainOriginalField(source)
  }
  const height = record(video)?.height
  for (const [key, fallback] of [
    ['fontFamily', undefined], ['fontSize', 0.09], ['fontWeight', 700], ['italic', false],
    ['strokeWidth', 0], ['tracking', 0], ['lineHeight', 1.2], ['backgroundRadius', 0.0135],
    ['backgroundPaddingX', 0.0315], ['backgroundPaddingY', 0.018], ['align', 'center'],
  ] as const) {
    if (editedText[key] === previousText[key]) continue
    const value = editedText[key] === undefined ? fallback : editedText[key]
    if (!style || clip.concatTitle) return undefined
    switch (key) {
      case 'fontFamily':
        if (typeof value !== 'string' || !['Noto Sans', 'Arial Unicode MS', 'DejaVu Sans', 'Arial'].includes(value)) return undefined
        style = { ...style, fontFamily: value }
        break
      case 'fontWeight':
        if (typeof value !== 'number' || !(value >= 100 && value <= 900)) return undefined
        style = { ...style, bold: value >= 600, fontWeight: value === 400 || value === 700 ? undefined : value }
        break
      case 'italic':
        if (typeof value !== 'boolean') return undefined
        style = { ...style, italic: value }
        break
      case 'lineHeight':
        if (typeof value !== 'number' || !Number.isFinite(value) || value < 0.5) return undefined
        style = { ...style, lineHeight: Math.fround(value) }
        break
      case 'align': {
        if (value !== 'left' && value !== 'center' && value !== 'right') return undefined
        const animation = clip.transform?.concatAnimation
        const changesPivot = animation && (animation.keys.scale || animation.keys.rotation ||
          animation.rest.rotationDegrees !== 0 || animation.rest.scaleX !== 1 || animation.rest.scaleY !== 1 ||
          animation.stretch.some(scale => scale !== 1))
        if (value !== 'center' && (changesPivot || Array.isArray(after.keys) && after.keys.some(key => {
          const property = record(key)?.property
          return property === 'scale' || property === 'rotation'
        }))) return undefined
        style = { ...style, align: value }
        break
      }
      case 'fontSize':
      case 'tracking':
      case 'backgroundRadius':
      case 'backgroundPaddingX':
      case 'backgroundPaddingY':
      case 'strokeWidth': {
        if (typeof value !== 'number' || typeof height !== 'number') return undefined
        const pixels = value * height
        if (key === 'fontSize') {
          if (!(pixels >= 1 && pixels <= 1000)) return undefined
          style = { ...style, fontSize: pixels }
        } else if (key === 'tracking') {
          style = { ...style, tracking: Math.fround(pixels) }
        } else if (key === 'backgroundRadius') {
          if (!(pixels >= 0 && pixels <= 1000)) return undefined
          style = { ...style, backgroundRadius: Math.fround(pixels) }
        } else if (key === 'backgroundPaddingX' || key === 'backgroundPaddingY') {
          if (!(pixels >= 0 && pixels <= 100000)) return undefined
          const [x, y] = style.backgroundPadding ?? [0, 0]
          const padding = Math.fround(pixels)
          style = { ...style, backgroundPadding: key === 'backgroundPaddingX' ? [padding, y] : [x, padding] }
        } else {
          const width = Math.round(pixels)
          if (!(width >= 0 && width <= 100 && Math.abs(width - pixels) < 1e-9)) return undefined
          style = { ...style, strokeWidth: width }
        }
        break
      }
    }
    retainOriginalField(key)
  }
  if (style?.tracking !== undefined) {
    const tracking = Math.fround(Math.fround(style.tracking * 1000) / Math.fround(style.fontSize))
    if (!(tracking >= -1000 && tracking <= 10000)) return undefined
  }
  if (style?.lineHeight !== undefined &&
      !(Math.fround(style.lineHeight * Math.fround(style.fontSize)) <= 5000)) return undefined

  const textBox = { textBoxWidth: clip.textBoxWidth, textBoxHeight: clip.textBoxHeight }
  for (const [source, target, axis] of [
    ['maxWidth', 'textBoxWidth', 'width'], ['maxHeight', 'textBoxHeight', 'height'],
  ] as const) {
    if (editedText[source] === previousText[source]) continue
    const dimension = record(video)?.[axis]
    const limit = editedText[source] === undefined ? 0 : editedText[source]
    if (clip.concatTitle || typeof dimension !== 'number' || typeof limit !== 'number') return undefined
    const pixels = Math.fround(limit * dimension)
    if (limit !== 0 && !(pixels >= 1 && pixels <= 100000)) return undefined
    textBox[target] = limit === 0 ? undefined : pixels
    retainOriginalField(source)
  }
  if (editedText.shadow !== previousText.shadow) {
    const shadow = editedText.shadow === undefined ? true : editedText.shadow
    if (clip.concatTitle || typeof shadow !== 'boolean' || !style?.shadow) return undefined
    if (shadow !== (previousText.shadow ?? true)) {
      style = shadow ? { ...style, concatShadow: true } : {
        ...style, ...(style.concatShadow ? { concatShadow: false } : {}), shadow: { ...style.shadow, alpha: 0 },
      }
    }
    retainOriginalField('shadow')
  }

  let transform = clip.transform
  const canvas = record(video)
  for (const [field, axis, dimension] of [['offsetX', 'x', 'width'], ['offsetY', 'y', 'height']] as const) {
    const previous = before[field] ?? 0
    const current = after[field] ?? 0
    if (previous === current) continue
    const size = canvas?.[dimension]
    const position = transform?.[axis]
    if (typeof previous !== 'number' || !Number.isFinite(previous) ||
        typeof current !== 'number' || !(Math.abs(current) <= 3) ||
        typeof size !== 'number' || !(size > 0) || !Number.isFinite(size) ||
        clip.concatTitle || transform?.position || position?.mode !== 'constant') return undefined
    const delta = current - previous
    const animation = transform.concatAnimation
    transform = {
      ...transform,
      [axis]: { ...position, value: position.value + delta * size },
      ...(animation ? { concatAnimation: { ...animation, rest: {
        ...animation.rest, [axis]: animation.rest[axis] + delta,
      } } } : {}),
    }
  }

  const poseFields = ['scale', 'stretchX', 'stretchY', 'rotation'] as const
  const previousPose = poseFields.map(field => before[field] ?? (field === 'rotation' ? 0 : 1))
  const editedPose = poseFields.map(field => after[field] ?? (field === 'rotation' ? 0 : 1))
  if (!sameJson(previousPose, editedPose)) {
    if (clip.concatTitle || !centeredCaption({ ...clip, style }) || !hasStaticCaptionPose(clip)) return undefined
    const [scale, stretchX, stretchY, rotation] = editedPose
    const [oldScale, oldStretchX, oldStretchY, oldRotation] = previousPose
    if (typeof scale !== 'number' || !(scale >= 0.05 && scale <= 8) ||
        typeof stretchX !== 'number' || !(stretchX >= 0.1 && stretchX <= 10) ||
        typeof stretchY !== 'number' || !(stretchY >= 0.1 && stretchY <= 10) ||
        typeof rotation !== 'number' || !Number.isFinite(rotation) ||
        typeof oldScale !== 'number' || !(oldScale > 0) ||
        typeof oldStretchX !== 'number' || !(oldStretchX > 0) ||
        typeof oldStretchY !== 'number' || !(oldStretchY > 0) ||
        typeof oldRotation !== 'number' || !Number.isFinite(oldRotation)) return undefined
    const sx = scale * stretchX / (oldScale * oldStretchX)
    const sy = scale * stretchY / (oldScale * oldStretchY)
    const turn = rotation - oldRotation
    const { scaleX, scaleY, rotationDegrees, concatAnimation } = transform
    if (scaleX.mode !== 'constant' || scaleY.mode !== 'constant' || rotationDegrees.mode !== 'constant') return undefined
    transform = {
      ...transform,
      scaleX: { mode: 'constant', value: scaleX.value * sx },
      scaleY: { mode: 'constant', value: scaleY.value * sy },
      rotationDegrees: { mode: 'constant', value: rotationDegrees.value + turn },
      ...(concatAnimation ? { concatAnimation: {
        ...concatAnimation, rest: {
          ...concatAnimation.rest,
          scaleX: concatAnimation.rest.scaleX * scale / oldScale,
          scaleY: concatAnimation.rest.scaleY * scale / oldScale,
          rotationDegrees: concatAnimation.rest.rotationDegrees + turn,
        },
        stretch: [
          concatAnimation.stretch[0] * stretchX / oldStretchX,
          concatAnimation.stretch[1] * stretchY / oldStretchY,
        ] as const,
      } } : {}),
    }
  }

  const previousCrop = normalizedCrop(before.crop)
  const editedCrop = normalizedCrop(after.crop)
  const previousFlipH = before.flipH ?? false
  const previousFlipV = before.flipV ?? false
  const flipH = after.flipH ?? false
  const flipV = after.flipV ?? false
  if (flipH !== previousFlipH || flipV !== previousFlipV || !sameJson(previousCrop, editedCrop)) {
    const width = canvas?.width
    const height = canvas?.height
    if (clip.concatTitle || !centeredCaption({ ...clip, style }) ||
        typeof flipH !== 'boolean' || typeof flipV !== 'boolean' ||
        typeof width !== 'number' || typeof height !== 'number') return undefined
    const edges = record(editedCrop)
    if (editedCrop !== null && !edges) return undefined
    if (edges && Object.keys(edges).some(edge => !cropEdges.includes(edge))) return undefined
    const amounts = cropEdges.map(edge => edges && Object.hasOwn(edges, edge) ? edges[edge] : 0)
    if (!amounts.every((amount): amount is number => typeof amount === 'number' && Number.isFinite(amount))) return undefined
    const [left = 0, top = 0, right = 0, bottom = 0] = amounts.map(amount => Math.max(0, Math.min(0.9, amount)))
    transform = {
      ...transform,
      concatPreparation: flipH || flipV || left || top || right || bottom
        ? { crop: [left, top, Math.min(right, 0.9 - left), Math.min(bottom, 0.9 - top)],
          canvas: [width, height], flipH, flipV } : undefined,
    }
  }

  let opacity = clip.opacity
  let opacityMultiplier = clip.opacityMultiplier
  const previousOpacity = before.opacity ?? 1
  const editedOpacity = after.opacity ?? 1
  const previousTitleOpacity = previousText.opacity ?? 1
  const editedTitleOpacity = editedText.opacity ?? 1
  if (editedOpacity !== previousOpacity || editedTitleOpacity !== previousTitleOpacity) {
    if (clip.concatTitle || typeof editedOpacity !== 'number' || !(editedOpacity >= 0 && editedOpacity <= 1) ||
        typeof editedTitleOpacity !== 'number' || !(editedTitleOpacity >= 0 && editedTitleOpacity <= 1)) return undefined
    const hasOpacityKeys = (keys: unknown) => Array.isArray(keys) && keys.some(key => record(key)?.property === 'opacity')
    const keyed = clip.opacity?.mode === 'keyframes' || !!transform?.concatAnimation?.keys.opacity?.length || hasOpacityKeys(after.keys)
    if (keyed || retainKeys) {
      // The title multiplier is applied after sampling and clamping the ride.
      if (editedOpacity !== previousOpacity && !retainKeys && !hasOpacityKeys(after.keys)) return undefined
      opacityMultiplier = editedTitleOpacity
    } else {
      if (opacity?.mode !== 'constant') return undefined
      const value = opacityMultiplier === undefined ? editedOpacity * editedTitleOpacity : editedOpacity
      if (opacityMultiplier !== undefined) opacityMultiplier = editedTitleOpacity
      opacity = { mode: 'constant', value }
      if (transform?.concatAnimation) {
        const animation = transform.concatAnimation
        transform = { ...transform, concatAnimation: {
          ...animation, rest: { ...animation.rest, opacity: value },
        } }
      }
    }
    if (editedTitleOpacity !== previousTitleOpacity) retainOriginalField('opacity')
  }

  const previousBlend = typeof before.blend === 'string' && before.blend ? before.blend : 'normal'
  const editedBlend = typeof after.blend === 'string' && after.blend ? after.blend : 'normal'
  let blendMode = clip.blendMode
  if (editedBlend !== previousBlend) {
    switch (editedBlend) {
      case 'add':
        blendMode = 'addition'
        break
      case 'normal':
      case 'multiply':
      case 'screen':
      case 'addition':
      case 'lighten':
      case 'darken':
        blendMode = editedBlend
        break
      default:
        return undefined
    }
  }

  // Text and whole-clip moves retain the original local clock. A static title
  // also accepts Concat's new duration and, for a head trim, its source in-point.
  // The imported document already carries the updated display name in clipNames.
  const unchangedFields = {
    ...after,
    start: before.start,
    name: before.name,
    ...(typeof after.trackId === 'string' && typeof before.trackId === 'string' ? { trackId: before.trackId } : {}),
    offsetX: before.offsetX ?? 0,
    offsetY: before.offsetY ?? 0,
    scale: before.scale ?? 1,
    stretchX: before.stretchX ?? 1,
    stretchY: before.stretchY ?? 1,
    rotation: before.rotation ?? 0,
    flipH: previousFlipH,
    flipV: previousFlipV,
    crop: previousCrop,
    opacity: previousOpacity,
    blend: previousBlend,
    ...(retainKeys ? { keys: before.keys ?? [] } : {}),
    ...(resized ? { duration: before.duration, sourceStart: before.sourceStart ?? 0 } : {}),
    text: comparisonText,
  }
  const comparison = { ...before, name: before.name }
  if (!sameJson(normalizedClip(comparison), normalizedClip(unchangedFields))) return undefined
  const start = Math.round(after.start * 1_000_000)
  const restored = {
    ...clip,
    text: editedText.content,
    style,
    transform,
    opacity,
    ...(blendMode === undefined ? {} : { blendMode }),
    ...(opacityMultiplier === undefined ? {} : { opacityMultiplier }),
    ...textBox,
    timelineStartTick: start,
    timelineEndTick: start + duration,
  }
  return retainKeys ? [restored, {
    offsetX: typeof after.offsetX === 'number' ? after.offsetX : 0,
    offsetY: typeof after.offsetY === 'number' ? after.offsetY : 0,
  }] : [restored]
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
  const timelines = new Map<string, { video: ReturnType<typeof captionCanvas>; clips: Map<string, unknown> }>()
  const clipLocations = new Map<string, string | null>()
  if (!unchanged) {
    const document = record(JSON.parse(await manifest.text()))
    const sharedVideo = record(document?.video)
    for (const value of Array.isArray(document?.timelines) ? document.timelines : []) {
      const timeline = record(value)
      if (!timeline || typeof timeline.id !== 'string' || !Array.isArray(timeline.clips)) continue
      const clips = new Map<string, unknown>()
      for (const entry of timeline.clips) {
        const clip = record(entry)
        if (typeof clip?.id !== 'string') continue
        clips.set(clip.id, clip)
        clipLocations.set(clip.id, clipLocations.has(clip.id) ? null : timeline.id)
      }
      const video = record(timeline.video ?? document?.video ?? {})
      if (video) timelines.set(timeline.id, { video: captionCanvas(video, sharedVideo), clips })
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
    const baselineVideo = record(baseline?.video)
    if (unchanged) {
      eligible.push([timelineId, clip])
      continue
    }
    const destinationId = timelines.get(timelineId)?.clips.has(clip.id) ? timelineId
      : baseline?.uniqueClipId === true ? clipLocations.get(clip.id) : undefined
    const destination = destinationId ? timelines.get(destinationId) : undefined
    if (!baseline || !baselineVideo || !destinationId || !destination ||
        !sameJson(captionCanvas(baselineVideo), destination.video)) continue
    const merged = mergeCaptionEdits(clip, baseline.clip, destination.clips.get(clip.id), destination.video)
    if (merged) eligible.push([destinationId, ...merged])
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
    const captionTracks = node.content.captionTracks ?? []
    const ids = new Set([...node.content.tracks, ...captionTracks].map(track => track.id))
    let available = MAX_COMPOSITION_TRACKS - ids.size
    let visibleLayers = node.content.tracks.filter(track => track.kind !== 'audio' && !track.hidden).length +
      captionTracks.filter(track => !track.hidden).length
    let serial = 1
    const tracks = node.content.tracks.flatMap<WireCompositionTrack>(track => {
      if (track.kind !== 'text') return [track]
      const recovered = new Set<string>()
      const clips = track.clips.map(clip => {
        const entry = group.get(clip.id)
        if (!entry) return clip
        const { clip: restored, importedAnimation } = entry
        if (restored.text !== clip.text || restored.timelineStartTick !== clip.timelineStartTick ||
            restored.timelineEndTick !== clip.timelineEndTick) {
          throw new Error('Клип субтитров расходится с манифестом шаблона')
        }
        group.delete(clip.id)
        recovered.add(clip.id)
        if (!importedAnimation) return restored
        const animation = clip.transform.concatAnimation
        const { x, y } = restored.transform
        if (!animation) {
          return { ...restored, opacity: clip.opacity, transform: { ...restored.transform, concatAnimation: undefined } }
        }
        if (x.mode !== 'constant' || y.mode !== 'constant' ||
            !Object.keys(animation.keys).every(key => captionKey(key, centeredCaption(restored)))) {
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
      if (!recovered.size || roles.has(track.id)) return [{ ...track, clips }]
      if (recovered.size === clips.length) {
        roles.add(track.id)
        return [{ ...track, clips }]
      }
      if (available <= 0 || (!track.hidden && visibleLayers >= MAX_COMPOSITION_VISIBLE_LAYERS)) {
        return [{ ...track, clips }]
      }
      let captionId: string
      do { captionId = `caption-lane-${serial++}` } while (ids.has(captionId))
      ids.add(captionId)
      roles.add(captionId)
      available--
      if (!track.hidden) visibleLayers++
      return [
        { ...track, clips: clips.filter(clip => !recovered.has(clip.id)) },
        { ...track, id: captionId, name: `${track.name} · Субтитры`, clips: clips.filter(clip => recovered.has(clip.id)) },
      ]
    })
    if (group.size) throw new Error('Клип субтитров отсутствует в шаблоне')
    compositions[id] = { ...node, content: { ...node.content, tracks, metadata: {
      ...node.content.metadata, captionTextTracks: [...roles],
    } } }
  }
  return { ...project, compositions }
}
