export type PreviewSessionState = 'Idle' | 'Ready' | 'Paused' | 'Playing' | 'Draining' | 'Failed'

export interface PreviewSessionSnapshot {
  readonly state: PreviewSessionState
  readonly revision: number
  readonly changedAtMs: number
  readonly error?: string
}

export interface MonotonicClock {
  now(): number
}

const browserClock: MonotonicClock = { now: () => performance.now() }

export class PreviewSession {
  private state: PreviewSessionState = 'Idle'
  private revision = 0
  private changedAtMs = 0
  private error: string | undefined

  constructor(private readonly clock: MonotonicClock = browserClock) {
    this.changedAtMs = clock.now()
  }

  snapshot(): PreviewSessionSnapshot {
    return { state: this.state, revision: this.revision, changedAtMs: this.changedAtMs, error: this.error }
  }

  ready(): PreviewSessionSnapshot {
    return this.transition('Ready')
  }

  play(): PreviewSessionSnapshot {
    if (this.state === 'Idle' || this.state === 'Draining' || this.state === 'Failed') return this.snapshot()
    return this.transition('Playing')
  }

  pause(): PreviewSessionSnapshot {
    if (this.state !== 'Playing' && this.state !== 'Ready') return this.snapshot()
    return this.transition('Paused')
  }

  drain(): PreviewSessionSnapshot {
    if (this.state === 'Idle') return this.snapshot()
    return this.transition('Draining')
  }

  drained(): PreviewSessionSnapshot {
    if (this.state !== 'Draining') return this.snapshot()
    return this.transition('Idle')
  }

  fail(error: string): PreviewSessionSnapshot {
    return this.transition('Failed', error || 'Ошибка предпросмотра')
  }

  reset(): PreviewSessionSnapshot {
    return this.transition('Idle')
  }

  private transition(state: PreviewSessionState, error?: string): PreviewSessionSnapshot {
    if (state === this.state && error === this.error) return this.snapshot()
    this.state = state
    this.error = error
    this.revision += 1
    this.changedAtMs = Math.max(this.changedAtMs, this.clock.now())
    return this.snapshot()
  }
}
