import type { FilmcraftExportMetadata } from '$lib/api'

export const exportMetadataFields: Array<{ key: keyof FilmcraftExportMetadata; label: string; multiline?: boolean }> = [
  { key: 'title', label: 'Название' },
  { key: 'creator', label: 'Автор' },
  { key: 'copyright', label: 'Авторские права' },
  { key: 'description', label: 'Описание', multiline: true },
  { key: 'comment', label: 'Комментарий', multiline: true },
]

export function validExportMetadata(metadata: FilmcraftExportMetadata) {
  const values = Object.values(metadata)
  const encoder = new TextEncoder()
  const sizes = values.map(value => encoder.encode(value).length)
  return values.every(value => !value.includes('\0'))
    && sizes.every(size => size <= 16384)
    && sizes.reduce((sum, size) => sum + size, 0) <= 65536
}
