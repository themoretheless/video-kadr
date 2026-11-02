/** Bounded serial request queue: continuous updates replace pending work without delaying it. */
export class LatestRequest<Input, Output> {
  private pending: { input: Input } | undefined
  private timer: ReturnType<typeof setTimeout> | undefined
  private active: AbortController | undefined
  private generation = 0
  constructor(
    private readonly run: (input: Input, signal: AbortSignal) => Promise<Output>,
    private readonly complete: (output: Output, input: Input) => void,
    private readonly fail: (error: unknown) => void,
    private readonly busy: (value: boolean) => void,
    private readonly interval = 120,
  ) {}
  push(input: Input): void {
    this.pending = { input }
    this.busy(true)
    this.schedule()
  }
  cancel(): void {
    this.generation++
    if (this.timer !== undefined) clearTimeout(this.timer)
    this.timer = undefined
    this.active?.abort()
    this.active = undefined
    this.pending = undefined
    this.busy(false)
  }
  private schedule(): void {
    if (this.timer !== undefined || this.active || !this.pending) return
    this.timer = setTimeout(() => {
      this.timer = undefined
      const work = this.pending
      if (!work) return
      this.pending = undefined
      const controller = new AbortController()
      this.active = controller
      const generation = this.generation
      Promise.resolve().then(() => this.run(work.input, controller.signal)).then(output => {
        if (generation === this.generation && !controller.signal.aborted) this.complete(output, work.input)
      }).catch(error => {
        if (generation === this.generation && !controller.signal.aborted) this.fail(error)
      }).finally(() => {
        if (generation !== this.generation) return
        this.active = undefined
        this.busy(Boolean(this.pending))
        this.schedule()
      })
    }, this.interval)
  }
}
