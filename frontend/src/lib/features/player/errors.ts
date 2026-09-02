export type PlaybackErrorKind = 'network' | 'media' | 'config' | 'unsupported'

export interface PlaybackError {
  readonly kind: PlaybackErrorKind
  readonly fatal: boolean
  readonly message: string
  readonly retryable: boolean
}

export interface RecoveryDecision {
  readonly action: 'retry' | 'fallback' | 'fail'
  readonly attempt: number
  readonly delayMs: number
}

export function classifyPlaybackError(input: unknown): PlaybackError {
  if (input instanceof DOMException && input.name === 'NotSupportedError') {
    return { kind: 'unsupported', fatal: true, retryable: false, message: input.message || 'Формат не поддерживается' }
  }
  const message = input instanceof Error ? input.message : typeof input === 'string' ? input : 'Ошибка воспроизведения'
  const normalized = message.toLowerCase()
  if (/network|fetch|offline|timeout|connection/.test(normalized)) {
    return { kind: 'network', fatal: false, retryable: true, message }
  }
  if (/decode|codec|media/.test(normalized)) {
    return { kind: 'media', fatal: true, retryable: false, message }
  }
  if (/config|manifest|source/.test(normalized)) {
    return { kind: 'config', fatal: true, retryable: false, message }
  }
  return { kind: 'media', fatal: true, retryable: false, message }
}

export class PlaybackRecoveryPolicy {
  private attempts = 0

  constructor(
    private readonly maxRetries = 2,
    private readonly baseDelayMs = 250,
  ) {}

  decide(error: PlaybackError, hasFallback: boolean): RecoveryDecision {
    if (error.retryable && this.attempts < this.maxRetries) {
      this.attempts += 1
      return { action: 'retry', attempt: this.attempts, delayMs: this.baseDelayMs * this.attempts }
    }
    return { action: hasFallback ? 'fallback' : 'fail', attempt: this.attempts, delayMs: 0 }
  }

  reset(): void {
    this.attempts = 0
  }
}
