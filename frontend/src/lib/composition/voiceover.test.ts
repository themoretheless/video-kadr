import { describe, expect, it } from 'vitest'
import { MAX_VOICEOVER_UPLOAD_BYTES, voiceoverFileValidationError } from './voiceover.js'

function fileWithSize(size: number, type = 'audio/webm'): File {
  const file = new File(['voice'], 'voiceover.webm', { type })
  Object.defineProperty(file, 'size', { configurable: true, value: size })
  return file
}

describe('composition voiceover upload boundary', () => {
  it('accepts a bounded audio recording', () => {
    expect(voiceoverFileValidationError(fileWithSize(32 * 1024))).toBeNull()
  })

  it('rejects empty, oversized, and non-audio files before upload', () => {
    expect(voiceoverFileValidationError(fileWithSize(0))).toContain('пуста')
    expect(voiceoverFileValidationError(fileWithSize(MAX_VOICEOVER_UPLOAD_BYTES + 1))).toContain('256 МиБ')
    expect(voiceoverFileValidationError(fileWithSize(10, 'video/webm'))).toContain('аудиофайлом')
  })
})
