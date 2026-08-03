export type ColorPrimariesV1 = 'bt709'
export type ColorTransferV1 = 'bt709' | 'srgb'
export type ColorMatrixV1 = 'bt709' | 'rgb'
export type ColorRangeV1 = 'limited' | 'full'
export type PixelModelV1 = 'yuv' | 'rgb'
export type ChromaLocationV1 = 'left' | 'center' | 'top_left'

export type UnsupportedColorReasonV1 =
  | 'missing_range' | 'missing_matrix' | 'missing_transfer' | 'missing_primaries'
  | 'missing_pixel_format' | 'unknown_range' | 'unknown_matrix' | 'unknown_transfer'
  | 'unknown_primaries' | 'unknown_pixel_format' | 'unknown_chroma_location'
  | 'bt601_unsupported' | 'hdr_unsupported' | 'wide_gamut_unsupported'
  | 'contradictory_metadata'

export interface SdrColorDescriptorV1 {
  primaries: ColorPrimariesV1
  transfer: ColorTransferV1
  matrix: ColorMatrixV1
  range: ColorRangeV1
  pixelModel: PixelModelV1
  chromaLocation?: ChromaLocationV1
}

export type ColorManagementProvenanceV1 = 'signaled' | 'legacy_assumed_bt709' | 'browser_decoded'

export type ColorManagementStatusV1 =
  | { status: 'supported'; descriptor: SdrColorDescriptorV1; provenance: ColorManagementProvenanceV1 }
  | { status: 'unsupported'; reason: UnsupportedColorReasonV1 }
  | { status: 'not_applicable' }

export interface ColorStatusPresentation {
  label: string
  warning: string | null
  verified: boolean
}

const REASONS: readonly UnsupportedColorReasonV1[] = [
  'missing_range', 'missing_matrix', 'missing_transfer', 'missing_primaries',
  'missing_pixel_format', 'unknown_range', 'unknown_matrix', 'unknown_transfer',
  'unknown_primaries', 'unknown_pixel_format', 'unknown_chroma_location',
  'bt601_unsupported', 'hdr_unsupported', 'wide_gamut_unsupported', 'contradictory_metadata',
]
const CHROMA: readonly ChromaLocationV1[] = ['left', 'center', 'top_left']
const PROVENANCE: readonly ColorManagementProvenanceV1[] = ['signaled', 'legacy_assumed_bt709', 'browser_decoded']

export const BROWSER_DECODED_SRGB_STATUS: Readonly<ColorManagementStatusV1> = Object.freeze({
  status: 'supported',
  provenance: 'browser_decoded',
  descriptor: Object.freeze({
    primaries: 'bt709', transfer: 'srgb', matrix: 'rgb', range: 'full', pixelModel: 'rgb',
  }),
})

export const FIXED_VIDEO_OUTPUT_REC709_LIMITED: Readonly<ColorManagementStatusV1> = Object.freeze({
  status: 'supported',
  provenance: 'signaled',
  descriptor: Object.freeze({
    primaries: 'bt709', transfer: 'bt709', matrix: 'bt709', range: 'limited', pixelModel: 'yuv', chromaLocation: 'left',
  }),
})

/** Strict parser for trusted boundaries and tests. It mirrors the Rust tagged enum. */
export function parseColorManagementStatusV1(value: unknown): ColorManagementStatusV1 {
  const record = object(value, 'color management status')
  if (record.status === 'not_applicable') {
    exactKeys(record, ['status'])
    return { status: 'not_applicable' }
  }
  if (record.status === 'unsupported') {
    exactKeys(record, ['status', 'reason'])
    if (!REASONS.includes(record.reason as UnsupportedColorReasonV1)) throw new Error('invalid unsupported color reason')
    return { status: 'unsupported', reason: record.reason as UnsupportedColorReasonV1 }
  }
  if (record.status !== 'supported') throw new Error('invalid color management status')
  exactKeys(record, ['status', 'descriptor', 'provenance'])
  if (!PROVENANCE.includes(record.provenance as ColorManagementProvenanceV1)) throw new Error('invalid color provenance')
  const descriptor = object(record.descriptor, 'SDR color descriptor')
  const allowed = descriptor.chromaLocation === undefined
    ? ['primaries', 'transfer', 'matrix', 'range', 'pixelModel']
    : ['primaries', 'transfer', 'matrix', 'range', 'pixelModel', 'chromaLocation']
  exactKeys(descriptor, allowed)
  if (descriptor.primaries !== 'bt709') throw new Error('invalid color primaries')
  if (!['bt709', 'srgb'].includes(String(descriptor.transfer))) throw new Error('invalid color transfer')
  if (!['bt709', 'rgb'].includes(String(descriptor.matrix))) throw new Error('invalid color matrix')
  if (!['limited', 'full'].includes(String(descriptor.range))) throw new Error('invalid color range')
  if (!['yuv', 'rgb'].includes(String(descriptor.pixelModel))) throw new Error('invalid pixel model')
  if (descriptor.chromaLocation !== undefined && !CHROMA.includes(descriptor.chromaLocation as ChromaLocationV1)) throw new Error('invalid chroma location')
  const result = descriptor as unknown as SdrColorDescriptorV1
  const coherent = result.pixelModel === 'yuv'
    ? result.matrix === 'bt709'
    : result.matrix === 'rgb' && result.range === 'full' && result.chromaLocation === undefined
  if (!coherent) throw new Error('contradictory color metadata')
  return { status: 'supported', descriptor: { ...result }, provenance: record.provenance as ColorManagementProvenanceV1 }
}

/** Never throws on malformed or forward-version API data. */
export function sourceColorStatus(value: unknown): ColorStatusPresentation {
  let status: ColorManagementStatusV1
  try { status = parseColorManagementStatusV1(value) } catch {
    return { label: 'Источник: цвет неизвестен', warning: 'Сервер вернул неизвестные цветовые метаданные; точное соответствие не гарантируется.', verified: false }
  }
  if (status.status === 'not_applicable') return { label: 'Источник: цвет N/A', warning: null, verified: true }
  if (status.status === 'unsupported') return {
    label: 'Источник: SDR не поддержан', warning: reasonLabel(status.reason), verified: false,
  }
  if (status.provenance === 'browser_decoded') return {
    label: 'Источник: browser sRGB', warning: 'Это декодированные браузером пиксели, а не проверенные цветовые метаданные файла.', verified: false,
  }
  const d = status.descriptor
  if (status.provenance === 'legacy_assumed_bt709') return {
    label: `Источник: ${d.transfer === 'srgb' ? 'sRGB' : 'Rec.709'} · ${d.range}`,
    warning: 'Rec.709 применён как legacy-допущение: исходник не содержал полного набора цветовых метаданных.',
    verified: false,
  }
  return { label: `Источник: ${d.transfer === 'srgb' ? 'sRGB' : 'Rec.709'} · ${d.range}`, warning: null, verified: true }
}

export function outputColorStatus(format: string): ColorStatusPresentation {
  switch (format.toLowerCase()) {
    case 'mp3': return { label: 'Экспорт: цвет N/A', warning: null, verified: true }
    case 'png':
    case 'jpg':
    case 'jpeg': return { label: 'Экспорт: sRGB full', warning: null, verified: true }
    case 'gif': return { label: 'Экспорт: GIF palette', warning: 'GIF имеет ограниченную палитру; оттенки и градиенты могут измениться.', verified: false }
    default: return { label: 'Экспорт: SDR Rec.709 limited', warning: null, verified: true }
  }
}

function object(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`invalid ${name}`)
  return value as Record<string, unknown>
}
function exactKeys(record: Record<string, unknown>, allowed: readonly string[]): void {
  if (Object.keys(record).length !== allowed.length || Object.keys(record).some(key => !allowed.includes(key))) throw new Error('unknown or missing color field')
}
function reasonLabel(reason: UnsupportedColorReasonV1): string {
  if (reason === 'hdr_unsupported' || reason === 'wide_gamut_unsupported') return 'HDR и wide-gamut пока не поддерживаются SDR-конвейером.'
  if (reason === 'bt601_unsupported') return 'Материал Rec.601 пока не поддерживается SDR-конвейером.'
  return 'Цветовые метаданные отсутствуют, неизвестны или противоречат друг другу.'
}
