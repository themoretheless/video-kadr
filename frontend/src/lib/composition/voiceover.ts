export const MAX_VOICEOVER_UPLOAD_BYTES = 256 * 1024 * 1024

/**
 * Voice recordings are uploaded only after MediaRecorder has finalized them.
 * Keep this check synchronous so an empty, non-audio, or unexpectedly large
 * Blob never reaches the shared multipart uploader.
 */
export function voiceoverFileValidationError(file: File): string | null {
  if (file.size <= 0) return 'Голосовая запись пуста'
  if (file.size > MAX_VOICEOVER_UPLOAD_BYTES) {
    return 'Голосовая запись превышает лимит 256 МиБ'
  }
  if (!file.type.toLowerCase().startsWith('audio/')) {
    return 'Запись микрофона должна быть аудиофайлом'
  }
  return null
}
