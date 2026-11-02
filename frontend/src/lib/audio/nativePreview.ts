import type { NativeNestedProject } from '../api'
import { renderCompositionAudioPreview } from '../api'
import { createPcmOutput } from './mediaGain'
const RATE = 48000
const BLOCK = 96000
export interface PreviewOutput { context: AudioContext; input: AudioNode; dispose(): void }
export type PreviewFetch = typeof renderCompositionAudioPreview
/** Two queued blocks maximum; one request at a time. AudioContext owns the clock. */
export class NativeAudioPreview {
  private epoch = 0
  private abort?: AbortController
  private output?: PreviewOutput
  private nodes = new Set<AudioBufferSourceNode>()
  private startSample = 0
  private nextSample = 0
  private endSample = 0
  private clockAnchor = 0
  private sampleAnchor = 0
  private scheduledUntil = 0
  private running = false
  private pending = false
  private project?: NativeNestedProject
  private token?: string
  private compositionId?: string
  error = ''
  constructor(private fetch: PreviewFetch = renderCompositionAudioPreview,
    private outputFactory: () => PreviewOutput = createPcmOutput) {}
  get finished(): boolean { return this.running && !this.pending && this.nextSample >= this.endSample && this.nodes.size === 0 }
  get ready(): boolean {
    const context = this.output?.context
    return this.running && !!context && context.state === 'running' && this.nodes.size > 0 &&
      context.currentTime >= this.clockAnchor && context.currentTime < this.scheduledUntil
  }
  timeUs(): number {
    const context = this.output?.context
    const sample = !context || this.nodes.size === 0 ? this.nextSample : Math.min(this.nextSample,
      this.sampleAnchor + Math.max(0, context.currentTime - this.clockAnchor) * RATE)
    return Math.round(Math.max(this.startSample, sample) * 1e6 / RATE)
  }
  start(project: NativeNestedProject, timeUs: number, durationUs: number, token?: string, compositionId?: string): void {
    this.stop()
    this.error = ''
    this.project = project
    this.token = token
    this.compositionId = compositionId
    this.startSample = Math.ceil(timeUs * RATE / 1e6)
    this.nextSample = this.startSample
    this.sampleAnchor = this.startSample
    // The server interval must remain inside the authored duration.
    this.endSample = Math.floor(durationUs * RATE / 1e6)
    this.running = true
    const epoch = this.epoch
    try { this.output = this.outputFactory() } catch (error) { this.fail(error, epoch); return }
    this.abort = new AbortController()
    void this.output.context.resume().then(() => {
      if (epoch === this.epoch) void this.pump(epoch)
    }).catch(error => this.fail(error, epoch))
  }
  stop(): void {
    this.epoch++
    this.running = false
    this.pending = false
    this.abort?.abort(); this.abort = undefined
    for (const node of this.nodes) { node.onended = null; node.stop(); node.disconnect() }
    this.nodes.clear()
    this.output?.dispose(); this.output = undefined
  }
  private fail(error: unknown, epoch: number): void {
    if (epoch !== this.epoch) return
    this.error = error instanceof Error ? error.message : String(error)
    this.stop()
  }
  private async pump(epoch: number): Promise<void> {
    if (!this.running || epoch !== this.epoch || this.pending || this.nodes.size >= 2 || this.nextSample >= this.endSample) return
    this.pending = true
    const start = this.nextSample
    const frames = Math.min(BLOCK, this.endSample - start)
    try {
      const samples = await this.fetch(this.project!, start, frames, this.token, this.abort!.signal, this.compositionId)
      if (epoch !== this.epoch || !this.running) return
      if (samples.length !== frames * 2) throw new Error('Неполный PCM-блок')
      const { context, input } = this.output!
      const buffer = context.createBuffer(2, frames, RATE)
      for (let ch = 0; ch < 2; ch++) {
        const channel = buffer.getChannelData(ch)
        for (let i = 0; i < frames; i++) channel[i] = samples[i * 2 + ch]
      }
      const when = this.nodes.size > 0 && this.scheduledUntil > context.currentTime
        ? this.scheduledUntil : context.currentTime + 0.03
      if (this.nodes.size === 0 || this.scheduledUntil <= context.currentTime) {
        this.sampleAnchor = start
        this.clockAnchor = when
      }
      const node = context.createBufferSource()
      node.buffer = buffer
      node.connect(input)
      node.onended = () => {
        if (epoch !== this.epoch) return
        this.nodes.delete(node); node.disconnect()
        void this.pump(epoch)
      }
      this.nodes.add(node)
      this.nextSample = start + frames
      this.scheduledUntil = when + frames / RATE
      node.start(when)
    } catch (error) { this.fail(error, epoch) }
    finally {
      if (epoch === this.epoch) { this.pending = false; void this.pump(epoch) }
    }
  }
}
