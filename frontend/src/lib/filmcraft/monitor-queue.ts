/** One in-flight preview plus the most recent requested position. Selection changes abort. */
export class MonitorQueue<Input, Output> {
  private key: string | null = null
  private epoch = 0
  private controller: AbortController | null = null
  private last: { input: Input } | null = null
  private next: { input: Input } | null = null
  constructor(private readonly load: (input: Input, signal: AbortSignal) => Promise<Output>, private readonly events: {
    reset: () => void; result: (value: Output) => void; error: (failure: unknown) => void; pending: (value: boolean) => void
  }, private readonly same: (a: Input, b: Input) => boolean = Object.is) {}
  submit(key: string, input: Input) {
    if (key === this.key && this.last && this.same(this.last.input, input)) return
    if (key !== this.key) { this.clear(); this.key = key }
    this.last = { input }
    if (this.controller) { this.next = { input }; return }
    this.start(input)
  }
  clear() {
    this.epoch++; this.controller?.abort(); this.controller = null; this.next = null; this.last = null; this.key = null
    this.events.reset(); this.events.pending(false)
  }
  private start(input: Input) {
    const epoch = this.epoch
    const controller = new AbortController(); this.controller = controller; this.events.pending(true)
    void Promise.resolve().then(() => this.load(input, controller.signal)).then(value => {
      if (epoch === this.epoch && !controller.signal.aborted) this.events.result(value)
    }).catch(failure => {
      if (epoch === this.epoch && !controller.signal.aborted && !this.next) this.events.error(failure)
    }).finally(() => {
      if (epoch !== this.epoch) return
      this.controller = null
      const next = this.next; this.next = null
      if (next) this.start(next.input)
      else this.events.pending(false)
    })
  }
}
