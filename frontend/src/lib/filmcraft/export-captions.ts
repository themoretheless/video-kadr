export const captionExtensions = ['srt', 'vtt', 'scc', 'mcc', 'stl', 'ttml', 'dfxp'] as const

export function readCaptionSidecar(result: unknown) {
  if (!result || typeof result !== 'object' || !('captionSidecar' in result)) return null
  const sidecar = result.captionSidecar
  if (!sidecar || typeof sidecar !== 'object' || !('extension' in sidecar) || !('bytesBase64' in sidecar)) return null
  const extension = captionExtensions.find(value => value === sidecar.extension)
  if (!extension || typeof sidecar.bytesBase64 !== 'string') return null
  return { extension, bytesBase64: sidecar.bytesBase64 }
}

export function captionSidecarBlob(sidecar: NonNullable<ReturnType<typeof readCaptionSidecar>>) {
  const binary = atob(sidecar.bytesBase64)
  const bytes = Uint8Array.from(binary, character => character.charCodeAt(0))
  const type = sidecar.extension === 'stl' ? 'application/octet-stream'
    : sidecar.extension === 'ttml' || sidecar.extension === 'dfxp' ? 'application/ttml+xml'
    : sidecar.extension === 'vtt' ? 'text/vtt' : 'text/plain'
  return new Blob([bytes], { type })
}
