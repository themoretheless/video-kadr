import { ApiError, createFilmcraftTrimSession, readFilmcraftTrimSession, commandFilmcraftTrimSession, closeFilmcraftTrimSession, type FilmcraftTrimSessionAction, type FilmcraftTrimSessionResult } from '$lib/api'

/** Serializes native session commands. A lost response requires reading authoritative state. */
export class FilmcraftTrimSessionClient {
  private tail: Promise<unknown> = Promise.resolve()
  private controller: AbortController | null = null
  private current: FilmcraftTrimSessionResult
  private uncertain = false
  private closed = false
  private constructor(private readonly token: string, initial: FilmcraftTrimSessionResult) { this.current = initial }
  static async create(token: string, input: Parameters<typeof createFilmcraftTrimSession>[1], signal?: AbortSignal) {
    return new FilmcraftTrimSessionClient(token, await createFilmcraftTrimSession(token, { ...input, creationKey: input.creationKey ?? crypto.randomUUID() }, signal))
  }
  get state() { return this.current }
  get needsRecovery() { return this.uncertain }
  get isClosed() { return this.closed }
  private enqueue<T>(work: () => Promise<T>): Promise<T> {
    const next = this.tail.then(work, work)
    this.tail = next.catch(() => undefined)
    return next
  }
  private async request<T>(work: (signal: AbortSignal) => Promise<T>): Promise<T> {
    const controller = new AbortController()
    this.controller = controller
    try {
      const result = await work(controller.signal)
      if (controller.signal.aborted) throw new Error('Ответ сессии отменён; требуется восстановить состояние')
      return result
    } catch (error) { this.uncertain = true; throw error }
    finally { if (this.controller === controller) this.controller = null }
  }
  cancelRequest() {
    if (this.controller) { this.uncertain = true; this.controller.abort() }
  }
  command(action: FilmcraftTrimSessionAction): Promise<FilmcraftTrimSessionResult> {
    return this.enqueue(async () => {
      if (this.closed) throw new Error('Сессия закрыта')
      if (this.uncertain) throw new Error('Требуется восстановить состояние сессии')
      const updated = await this.request(signal => commandFilmcraftTrimSession(this.token, { sessionId: this.current.sessionId, revision: this.current.revision, action }, signal))
      this.current = updated
      return updated
    })
  }
  recover(): Promise<FilmcraftTrimSessionResult | null> {
    return this.enqueue(async () => {
      if (this.closed) return null
      try {
        const updated = await this.request(signal => readFilmcraftTrimSession(this.token, this.current.sessionId, signal))
        this.current = updated; this.uncertain = false
        return updated
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) { this.closed = true; this.uncertain = false; return null }
        throw error
      }
    })
  }
  close(): Promise<void> {
    return this.enqueue(async () => {
      if (this.closed) return
      if (this.uncertain) throw new Error('Требуется восстановить состояние сессии')
      await this.request(signal => closeFilmcraftTrimSession(this.token, this.current.sessionId, this.current.revision, signal))
      this.closed = true
    })
  }
}
