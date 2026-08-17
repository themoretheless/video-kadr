// Asset library: images, audio, video, fonts and subtitle files that overlays,
// titles and the audio mixer reference by id. The upload flow mirrors the LUT
// upload in store.ts: validate locally first, keep one in-flight request, and
// surface a Russian error string next to the control that started it.

import { reactive } from 'vue'
import * as api from '../api'
import { toast } from '../toasts'
import type { AssetEntry, AssetKind } from '../types'
import { assetIdOr, clampNullable, enumOr, isRecord, textOr } from './validation'

export const ASSET_KINDS: readonly AssetKind[] = ['image', 'audio', 'video', 'font', 'subtitle']

/** Body caps from the feature contract, section 4. */
const MEDIA_UPLOAD_LIMIT = 64 * 1024 * 1024
const DOCUMENT_UPLOAD_LIMIT = 4 * 1024 * 1024

export const MAX_ASSET_UPLOAD_BYTES: Record<AssetKind, number> = {
  image: MEDIA_UPLOAD_LIMIT,
  audio: MEDIA_UPLOAD_LIMIT,
  video: MEDIA_UPLOAD_LIMIT,
  font: DOCUMENT_UPLOAD_LIMIT,
  subtitle: DOCUMENT_UPLOAD_LIMIT,
}

const KIND_LABELS: Record<AssetKind, string> = {
  image: 'изображение',
  audio: 'аудио',
  video: 'видео',
  font: 'шрифт',
  subtitle: 'субтитры',
}

export interface AssetsState {
  list: AssetEntry[]
  loading: boolean
  loadError: string
  uploading: boolean
  uploadError: string
  /** 0..100 while a body is being sent, null when nothing is in flight. */
  uploadProgress: number | null
}

export const assetsState = reactive<AssetsState>({
  list: [],
  loading: false,
  loadError: '',
  uploading: false,
  uploadError: '',
  uploadProgress: null,
})

/**
 * Validate a server response before it reaches state. The API layer types the
 * response optimistically, so this is the only place an `AssetEntry` is trusted.
 */
export function sanitizeAssetEntry(value: unknown): AssetEntry | null {
  if (!isRecord(value)) return null
  // Same allow-list the specs use, so an id stored here always survives the
  // sanitizers in overlays/audioMix later.
  const id = assetIdOr(value.id, null)
  if (!id) return null
  if (typeof value.kind !== 'string') return null
  if (!ASSET_KINDS.includes(value.kind as AssetKind)) return null
  const sizeBytes = clampNullable(value.sizeBytes, 0, Number.MAX_SAFE_INTEGER)
  if (sizeBytes === null) return null
  return {
    id,
    kind: enumOr(value.kind, ASSET_KINDS, 'image'),
    filename: textOr(value.filename, '', 255),
    mime: textOr(value.mime, '', 255),
    sizeBytes,
    sha256: textOr(value.sha256, '', 64),
    width: clampNullable(value.width, 0, 100000),
    height: clampNullable(value.height, 0, 100000),
    duration: clampNullable(value.duration, 0, 24 * 60 * 60),
  }
}

/** Read-only URL of a stored asset, for previews. */
export function assetUrl(asset: AssetEntry): string {
  return `/files/assets/${encodeURIComponent(asset.id)}`
}

export function assetsOfKind(kind: AssetKind): AssetEntry[] {
  return assetsState.list.filter((asset) => asset.kind === kind)
}

export function findAsset(id: string | null): AssetEntry | null {
  if (!id) return null
  return assetsState.list.find((asset) => asset.id === id) ?? null
}

function uploadValidationError(file: File, kind: AssetKind): string | null {
  if (file.size <= 0) return 'Файл пуст'
  const limit = MAX_ASSET_UPLOAD_BYTES[kind]
  if (file.size > limit) {
    return `Файл больше лимита ${Math.round(limit / (1024 * 1024))} МБ`
  }
  return null
}

function describeError(error: unknown): string {
  if (error instanceof api.AssetsRequireServerError) return error.message
  if (error instanceof api.BackendUnavailableError) return error.message
  if (error instanceof api.ApiError) return error.message
  if (error instanceof Error) return error.message
  return String(error)
}

/** Replace the list with what the server has. Failures surface in `loadError`. */
export async function loadAssets(): Promise<void> {
  if (assetsState.loading) return
  assetsState.loading = true
  assetsState.loadError = ''
  try {
    const entries = await api.getAssets()
    assetsState.list = entries
      .map(sanitizeAssetEntry)
      .filter((asset): asset is AssetEntry => asset !== null)
  } catch (error) {
    // Non-fatal: the asset pickers just stay empty.
    assetsState.list = []
    assetsState.loadError = describeError(error)
  } finally {
    assetsState.loading = false
  }
}

/**
 * Upload one asset and prepend it to the list. Returns the stored entry, or
 * null when validation, the network or the server rejected it.
 */
export async function uploadAsset(
  file: File,
  kind: AssetKind,
  signal?: AbortSignal,
): Promise<AssetEntry | null> {
  if (assetsState.uploading) return null
  const validationError = uploadValidationError(file, kind)
  if (validationError) {
    assetsState.uploadError = validationError
    toast('error', validationError)
    return null
  }

  assetsState.uploading = true
  assetsState.uploadError = ''
  assetsState.uploadProgress = 0
  try {
    const raw = await api.uploadAsset(file, kind, {
      signal,
      onProgress: (percent) => {
        assetsState.uploadProgress = percent
      },
    })
    const asset = sanitizeAssetEntry(raw)
    if (!asset) throw new Error('Сервер вернул некорректные данные ассета')
    assetsState.list = [asset, ...assetsState.list.filter((entry) => entry.id !== asset.id)]
    toast('success', `Загружено: ${asset.filename || KIND_LABELS[kind]}`)
    return asset
  } catch (error) {
    if (error instanceof Error && error.message === 'cancelled') {
      toast('info', 'Загрузка отменена')
      return null
    }
    assetsState.uploadError = describeError(error)
    toast('error', assetsState.uploadError)
    return null
  } finally {
    assetsState.uploading = false
    assetsState.uploadProgress = null
  }
}

/** Delete a stored asset. The list drops it even if the server 404s. */
export async function deleteAsset(id: string): Promise<boolean> {
  try {
    await api.deleteAsset(id)
    assetsState.list = assetsState.list.filter((asset) => asset.id !== id)
    toast('info', 'Ассет удалён')
    return true
  } catch (error) {
    toast('error', describeError(error))
    return false
  }
}

export function resetAssets(): void {
  assetsState.list = []
  assetsState.loading = false
  assetsState.loadError = ''
  assetsState.uploading = false
  assetsState.uploadError = ''
  assetsState.uploadProgress = null
}
