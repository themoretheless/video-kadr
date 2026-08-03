export const AUDIO_SYNC_SECONDS = 20
export const AUDIO_SYNC_HZ = 50
export const AUDIO_SYNC_MAX_SAMPLES = AUDIO_SYNC_SECONDS * AUDIO_SYNC_HZ

/** Preserve the container clock with `-copyts`, then anchor the resampler at
 * PTS zero. If the first audio packet has positive PTS, swresample emits zeros
 * until that packet instead of rebasing its content to sample zero. */
export function browserAudioSyncExtractionArgs(input: string, output: string): string[] {
  if (!input || !output || /[\0\r\n]/.test(input) || /[\0\r\n]/.test(output)) throw new Error('invalid audio sync path')
  return [
    '-v', 'error', '-copyts', '-i', input, '-t', String(AUDIO_SYNC_SECONDS),
    '-map', '0:a:0', '-vn',
    '-af', `aformat=channel_layouts=mono,aeval=abs(val(0)),aresample=${AUDIO_SYNC_HZ}:async=1:first_pts=0,atrim=end_sample=${AUDIO_SYNC_MAX_SAMPLES}`,
    '-ac', '1', '-ar', String(AUDIO_SYNC_HZ), '-f', 'f32le', output,
  ]
}
