export const BRAND_KIT_SCHEMA_VERSION = 1 as const

export interface BrandAssetMetadata {
  assetRef: string
  fingerprint: string
  mimeType: string
  byteLength: number
}

export interface BrandColor { id: string; name: string; value: { space: 'srgb'; rgba: [number, number, number, number] } }
export interface BrandFont { id: string; name: string; asset: BrandAssetMetadata; family: string; weight: number; style: 'normal' | 'italic'; license?: string }
export interface BrandLogo { id: string; name: string; asset: BrandAssetMetadata; variant: 'primary' | 'dark' | 'light' | 'mark'; safeArea?: number }
export interface BrandKit {
  schemaVersion: typeof BRAND_KIT_SCHEMA_VERSION
  id: string
  revision: number
  name: string
  colors: BrandColor[]
  fonts: BrandFont[]
  logos: BrandLogo[]
  defaults: { primaryColorId?: string; headingFontId?: string; bodyFontId?: string; primaryLogoId?: string }
}

const ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/
const SHA256 = /^[a-f0-9]{64}$/
const FONT_MIMES = new Set(['font/woff', 'font/woff2'])
const LOGO_MIMES = new Set(['image/png', 'image/webp', 'image/svg+xml'])

function record(value: unknown, field: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`invalid ${field}`)
  return value as Record<string, unknown>
}
function exact(value: Record<string, unknown>, allowed: readonly string[], field: string): void {
  const unexpected = Object.keys(value).find(key => !allowed.includes(key))
  if (unexpected) throw new Error(`invalid ${field}: unexpected ${unexpected}`)
}
function id(value: unknown, field: string): asserts value is string {
  if (typeof value !== 'string' || !ID.test(value)) throw new Error(`invalid ${field}`)
}
function label(value: unknown, field: string): asserts value is string {
  if (typeof value !== 'string' || value !== value.trim() || !value || [...value].length > 256 || /\p{Cc}/u.test(value)) throw new Error(`invalid ${field}`)
}
function asset(value: unknown, field: string, mimes: Set<string>): asserts value is BrandAssetMetadata {
  const v = record(value, field); exact(v, ['assetRef', 'fingerprint', 'mimeType', 'byteLength'], field)
  id(v.assetRef, `${field}.assetRef`)
  if (typeof v.fingerprint !== 'string' || !SHA256.test(v.fingerprint) || typeof v.mimeType !== 'string' || !mimes.has(v.mimeType)
    || !Number.isSafeInteger(v.byteLength) || Number(v.byteLength) <= 0 || Number(v.byteLength) > 32 * 1024 * 1024) throw new Error(`invalid ${field}`)
}

export function validateBrandKit(value: BrandKit): void {
  const root = record(value, 'brand kit'); exact(root, ['schemaVersion', 'id', 'revision', 'name', 'colors', 'fonts', 'logos', 'defaults'], 'brand kit')
  if (value.schemaVersion !== 1) throw new Error(`unsupported brand kit schemaVersion ${String(value.schemaVersion)}`)
  id(value.id, 'brand kit id'); label(value.name, 'brand kit name')
  if (!Number.isSafeInteger(value.revision) || value.revision < 1) throw new Error('invalid brand kit revision')
  if (!Array.isArray(value.colors) || value.colors.length > 256 || !Array.isArray(value.fonts) || value.fonts.length > 64 || !Array.isArray(value.logos) || value.logos.length > 128) throw new Error('invalid brand kit collections')
  const ids = new Set<string>(), colorIds = new Set<string>(), fontIds = new Set<string>(), logoIds = new Set<string>()
  for (const color of value.colors) {
    const v = record(color, 'brand color'); exact(v, ['id', 'name', 'value'], 'brand color'); id(color.id, 'brand color id'); label(color.name, 'brand color name')
    const c = record(color.value, 'brand color value'); exact(c, ['space', 'rgba'], 'brand color value')
    if (color.value.space !== 'srgb' || !Array.isArray(color.value.rgba) || color.value.rgba.length !== 4 || color.value.rgba.some(n => typeof n !== 'number' || !Number.isFinite(n) || n < 0 || n > 1 || Object.is(n, -0))) throw new Error('invalid brand color value')
    if (ids.has(color.id)) throw new Error(`duplicate brand id ${color.id}`); ids.add(color.id); colorIds.add(color.id)
  }
  for (const font of value.fonts) {
    const v = record(font, 'brand font'); exact(v, ['id', 'name', 'asset', 'family', 'weight', 'style', 'license'], 'brand font'); id(font.id, 'brand font id'); label(font.name, 'brand font name'); label(font.family, 'brand font family'); asset(font.asset, 'brand font asset', FONT_MIMES)
    if (!Number.isInteger(font.weight) || font.weight < 100 || font.weight > 900 || font.weight % 100 || !['normal', 'italic'].includes(font.style) || (font.license !== undefined && (typeof font.license !== 'string' || font.license.length > 1024))) throw new Error('invalid brand font')
    if (ids.has(font.id)) throw new Error(`duplicate brand id ${font.id}`); ids.add(font.id); fontIds.add(font.id)
  }
  for (const logo of value.logos) {
    const v = record(logo, 'brand logo'); exact(v, ['id', 'name', 'asset', 'variant', 'safeArea'], 'brand logo'); id(logo.id, 'brand logo id'); label(logo.name, 'brand logo name'); asset(logo.asset, 'brand logo asset', LOGO_MIMES)
    if (!['primary', 'dark', 'light', 'mark'].includes(logo.variant) || (logo.safeArea !== undefined && (!Number.isFinite(logo.safeArea) || logo.safeArea < 0 || logo.safeArea > 1))) throw new Error('invalid brand logo')
    if (ids.has(logo.id)) throw new Error(`duplicate brand id ${logo.id}`); ids.add(logo.id); logoIds.add(logo.id)
  }
  const defaults = record(value.defaults, 'brand defaults'); exact(defaults, ['primaryColorId', 'headingFontId', 'bodyFontId', 'primaryLogoId'], 'brand defaults')
  const expected = { primaryColorId: colorIds, headingFontId: fontIds, bodyFontId: fontIds, primaryLogoId: logoIds }
  for (const [key, candidate] of Object.entries(defaults)) if (candidate !== undefined && (typeof candidate !== 'string' || !expected[key as keyof typeof expected].has(candidate))) throw new Error(`invalid brand default ${key}`)
}

export function migrateBrandKit(value: unknown): BrandKit {
  const raw = record(value, 'brand kit')
  if (raw.schemaVersion !== BRAND_KIT_SCHEMA_VERSION) throw new Error(`unsupported brand kit schemaVersion ${String(raw.schemaVersion)}`)
  const result = structuredClone(raw) as unknown as BrandKit
  validateBrandKit(result)
  return result
}

export function nextBrandKitRevision(previous: BrandKit, update: Omit<BrandKit, 'schemaVersion' | 'id' | 'revision'>): BrandKit {
  validateBrandKit(previous)
  if (previous.revision === Number.MAX_SAFE_INTEGER) throw new Error('brand kit revision overflow')
  const result: BrandKit = { ...structuredClone(update), schemaVersion: 1, id: previous.id, revision: previous.revision + 1 }
  validateBrandKit(result)
  return result
}
