import { reactive } from 'vue'
import {
  deleteBrandKit as deleteStoredBrandKit,
  deleteDesignAsset,
  deleteProjectTemplate,
  getBrandKit,
  getDesignAsset,
  getProjectTemplate,
  listBrandKits,
  listProjectTemplates,
  putDesignAsset,
  saveBrandKit,
  saveProjectTemplate,
} from './browser-design-store'
import { type BrandKit } from './domain/brand-kit'
import {
  instantiateProjectTemplate,
  type PlaceholderBinding,
  type ProjectTemplate,
  type TemplatePlaceholder,
} from './domain/project-template'
import { openInstantiatedProject, timelineState } from './store'

export interface PlaceholderDraft { label: string; kind: 'video' | 'audio' | 'text' | 'logo'; required: boolean }
export interface BrandKitDraft {
  name: string
  colors: Array<{ name: string; value: string }>
  fontFiles: File[]
  logoFiles: File[]
}

export const designState = reactive({
  templates: [] as ProjectTemplate[],
  kits: [] as BrandKit[],
  busy: false,
  error: '',
  notice: '',
})

let operation = 0
const token = (prefix: string) => `${prefix}-${crypto.randomUUID()}`

function errorMessage(error: unknown): string { return error instanceof Error ? error.message : String(error) }
function canonicalJson(value: unknown): string {
  const normalize = (item: unknown): unknown => Array.isArray(item) ? item.map(normalize)
    : item && typeof item === 'object' ? Object.fromEntries(Object.entries(item as Record<string, unknown>).sort(([left], [right]) => left.localeCompare(right)).map(([key, nested]) => [key, normalize(nested)])) : item
  return JSON.stringify(normalize(value))
}

async function run<T>(work: () => Promise<T>, notice: string): Promise<T | null> {
  if (designState.busy) return null
  const current = ++operation
  designState.busy = true; designState.error = ''; designState.notice = ''
  try {
    const result = await work()
    if (current === operation) designState.notice = notice
    return result
  } catch (error) {
    if (current === operation) designState.error = errorMessage(error)
    return null
  } finally { if (current === operation) designState.busy = false }
}

export async function loadDesignCatalog(): Promise<void> {
  const current = ++operation
  designState.busy = true; designState.error = ''
  try {
    const [templates, kits] = await Promise.all([listProjectTemplates(), listBrandKits()])
    if (current !== operation) return
    designState.templates = templates.map(row => row.value)
    designState.kits = kits.map(row => row.value)
  } catch (error) { if (current === operation) designState.error = errorMessage(error) }
  finally { if (current === operation) designState.busy = false }
}

function findEffect(kind: 'text' | 'logo', used: Set<string>) {
  const accepted = kind === 'text' ? new Set(['text-v1', 'title-v1']) : new Set(['graphic-v1', 'logo-v1'])
  return timelineState.document?.sequences.flatMap(sequence => sequence.tracks)
    .flatMap(track => track.clips).flatMap(clip => clip.effects)
    .find(effect => accepted.has(effect.kind) && !used.has(effect.id))
}

export async function saveCurrentProjectTemplate(name: string, drafts: PlaceholderDraft[], brandKitId?: string): Promise<void> {
  await run(async () => {
    const document = timelineState.document
    if (!document) throw new Error('Откройте проект перед созданием шаблона')
    const mediaUsed = new Set<string>(), effectsUsed = new Set<string>()
    const placeholders: TemplatePlaceholder[] = drafts.map((draft, index) => {
      const id = `slot-${index + 1}-${crypto.randomUUID()}`
      if (draft.kind === 'video' || draft.kind === 'audio') {
        const media = document.media.find(item => item.kind === draft.kind && !mediaUsed.has(item.id))
        if (!media) throw new Error(`Нет свободного ${draft.kind === 'video' ? 'видео' : 'аудио'}-источника для placeholder «${draft.label}»`)
        mediaUsed.add(media.id)
        return { id, name: draft.label, kind: 'media', target: { mediaId: media.id }, required: draft.required, constraints: { mediaKind: draft.kind } }
      }
      const effect = findEffect(draft.kind, effectsUsed)
      if (!effect) throw new Error(`В проекте нет ${draft.kind === 'text' ? 'текстового' : 'графического'} overlay для placeholder «${draft.label}»`)
      effectsUsed.add(effect.id)
      return draft.kind === 'text'
        ? { id, name: draft.label, kind: 'text', target: { effectId: effect.id, parameter: 'text' }, required: draft.required }
        : { id, name: draft.label, kind: 'logo', target: { effectId: effect.id, parameter: 'assetRef' }, required: draft.required }
    })
    const brandKit = brandKitId ? designState.kits.find(item => item.id === brandKitId) : undefined
    if (brandKitId && !brandKit) throw new Error('Выбранный Brand kit больше недоступен')
    const mediaAssets = document.media.map(media => {
      const assetRef = media.assetRef, fingerprint = media.contentFingerprint
      const byteLength = media.metadata.sizeBytes, mimeType = media.metadata.fileType
      if (typeof assetRef !== 'string' || typeof fingerprint !== 'string' || typeof byteLength !== 'number' || !Number.isSafeInteger(byteLength) || byteLength <= 0) throw new Error(`У медиа «${media.id}» нет полной канонической зависимости`)
      return { kind: 'media' as const, assetRef, fingerprint, mimeType: typeof mimeType === 'string' && mimeType ? mimeType : 'application/octet-stream', byteLength }
    })
    const designAssets = brandKit ? [
      ...brandKit.fonts.map(font => ({ kind: 'font' as const, ...font.asset })),
      ...brandKit.logos.map(logo => ({ kind: 'logo' as const, ...logo.asset })),
    ] : []
    const template: ProjectTemplate = {
      schemaVersion: 1, id: token('template'), revision: 1, name: name.trim(),
      sourceProjectSchemaVersion: 4, document: structuredClone(document), placeholders,
      requiredAssets: [...mediaAssets, ...designAssets],
      ...(brandKit ? { brandKitPin: { id: brandKit.id, revision: brandKit.revision }, brandKitSnapshot: structuredClone(brandKit) } : {}),
    }
    const saved = await saveProjectTemplate(template, 0)
    designState.templates = [saved.value, ...designState.templates]
  }, 'Шаблон сохранён на этом устройстве')
}

function defaultBindings(template: ProjectTemplate): Record<string, PlaceholderBinding> {
  const result: Record<string, PlaceholderBinding> = {}
  for (const placeholder of template.placeholders) {
    if (placeholder.kind === 'media') {
      const media = template.document.media.find(item => item.id === placeholder.target.mediaId)!
      if (!media.assetRef || !media.contentFingerprint) { if (placeholder.required) throw new Error(`Нет исходника для «${placeholder.name}»`); continue }
      result[placeholder.id] = { kind: 'media', media: { assetRef: media.assetRef, contentFingerprint: media.contentFingerprint, kind: media.kind as 'video' | 'audio' | 'image', metadata: structuredClone(media.metadata) } }
    } else {
      const effect = template.document.sequences.flatMap(sequence => sequence.tracks).flatMap(track => track.clips).flatMap(clip => clip.effects).find(item => item.id === placeholder.target.effectId)!
      if (placeholder.kind === 'text') result[placeholder.id] = { kind: 'text', text: String(effect.parameters.text ?? '') }
      else {
        const assetRef = effect.parameters.assetRef, fingerprint = effect.parameters.contentFingerprint, mimeType = effect.parameters.mimeType, byteLength = effect.parameters.byteLength
        if (typeof assetRef !== 'string' || typeof fingerprint !== 'string' || !['image/png','image/webp','image/svg+xml'].includes(String(mimeType)) || typeof byteLength !== 'number') {
          if (placeholder.required) throw new Error(`Нет логотипа для «${placeholder.name}»`)
          continue
        }
        result[placeholder.id] = { kind: 'logo', assetRef, fingerprint, mimeType: mimeType as 'image/png'|'image/webp'|'image/svg+xml', byteLength }
      }
    }
  }
  return result
}

export async function instantiateTemplate(id: string): Promise<void> {
  await run(async () => {
    const row = await getProjectTemplate(id)
    if (!row) throw new Error('Шаблон не найден или повреждён')
    if (row.value.brandKitPin) {
      const kit = await getBrandKit(row.value.brandKitPin.id, row.value.brandKitPin.revision)
      if (!kit || kit.revision !== row.value.brandKitPin.revision) throw new Error('Закреплённая ревизия Brand kit недоступна')
      if (canonicalJson(kit.value) !== canonicalJson(row.value.brandKitSnapshot)) throw new Error('Snapshot закреплённого Brand kit не совпадает с каталогом')
    }
    for (const dependency of row.value.requiredAssets.filter(asset => asset.kind !== 'media')) {
      const stored = await getDesignAsset(dependency.assetRef)
      if (!stored || stored.manifest.fingerprint !== dependency.fingerprint || stored.manifest.byteLength !== dependency.byteLength || stored.manifest.mimeType !== dependency.mimeType) throw new Error(`Design asset ${dependency.assetRef} недоступен или повреждён`)
    }
    const document = instantiateProjectTemplate(row.value, defaultBindings(row.value), () => token('entity'))
    openInstantiatedProject(document)
  }, 'Создан новый проект из шаблона')
}

function normalizedFile(file: File, kind: 'font' | 'logo'): File {
  const extension = file.name.split('.').pop()?.toLowerCase()
  const type = kind === 'font'
    ? (extension === 'woff2' ? 'font/woff2' : extension === 'woff' ? 'font/woff' : file.type)
    : (extension === 'png' ? 'image/png' : extension === 'webp' ? 'image/webp' : file.type)
  return new File([file], file.name, { type, lastModified: file.lastModified })
}

function rgba(hex: string): [number, number, number, number] {
  const value = hex.slice(1)
  return [0, 2, 4].map(index => Number.parseInt(value.slice(index, index + 2), 16) / 255).concat(1) as [number, number, number, number]
}

export async function createBrandKit(draft: BrandKitDraft): Promise<void> {
  await run(async () => {
    const fonts = [] as BrandKit['fonts'], logos = [] as BrandKit['logos']
    const uploaded: string[] = []
    try {
      for (const file of draft.fontFiles) {
        const id = token('font'), manifest = await putDesignAsset(id, file.name, 'font', normalizedFile(file, 'font'))
        uploaded.push(id)
        const asset = { assetRef: manifest.id, fingerprint: manifest.fingerprint, mimeType: manifest.mimeType, byteLength: manifest.byteLength }
        fonts.push({ id, name: file.name, family: file.name.replace(/\.[^.]+$/, ''), weight: 400, style: 'normal', asset })
      }
      for (const [index, file] of draft.logoFiles.entries()) {
        const id = token('logo'), manifest = await putDesignAsset(id, file.name, 'logo', normalizedFile(file, 'logo'))
        uploaded.push(id)
        const asset = { assetRef: manifest.id, fingerprint: manifest.fingerprint, mimeType: manifest.mimeType, byteLength: manifest.byteLength }
        logos.push({ id, name: file.name, variant: index === 0 ? 'primary' : 'mark', asset })
      }
      const colors = draft.colors.map(item => ({ id: token('color'), name: item.name, value: { space: 'srgb' as const, rgba: rgba(item.value) } }))
      const kit: BrandKit = {
        schemaVersion: 1, id: token('brand'), revision: 1, name: draft.name.trim(), colors, fonts, logos,
        defaults: { primaryColorId: colors[0]?.id, headingFontId: fonts[0]?.id, bodyFontId: fonts[0]?.id, primaryLogoId: logos[0]?.id },
      }
      const saved = await saveBrandKit(kit, 0)
      designState.kits = [saved.value, ...designState.kits]
    } catch (error) {
      await Promise.allSettled(uploaded.map(id => deleteDesignAsset(id)))
      throw error
    }
  }, 'Brand kit сохранён на этом устройстве')
}

export async function deleteTemplate(id: string): Promise<void> {
  await run(async () => { await deleteProjectTemplate(id); designState.templates = designState.templates.filter(item => item.id !== id) }, 'Шаблон удалён')
}
export async function deleteBrandKit(id: string): Promise<void> {
  await run(async () => { await deleteStoredBrandKit(id); designState.kits = designState.kits.filter(item => item.id !== id) }, 'Brand kit удалён')
}
