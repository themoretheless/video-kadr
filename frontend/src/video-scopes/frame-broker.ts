export type ScopeTapId = 'pre-grade' | 'post-grade'
export type ScopeAccuracy = 'exact-live' | 'exact-paused'
export type ScopeSourceMode = 'original' | 'proxy'

export interface ScopeFrameIdentity {
  sourceFingerprint: string
  timelineTick: number
  graphVersion: string
  tapId: ScopeTapId
  colorDescriptorId: 'straight-rgba8-encoded-srgb'
  width: number
  height: number
  sourceMode: ScopeSourceMode
  mappingIdentity: string
}

export interface ScopeFrame {
  identity: ScopeFrameIdentity
  accuracy: ScopeAccuracy
  rgba: Uint8ClampedArray
}

export interface ScopeUnavailable {
  tapId: ScopeTapId
  generation: number
  reason: string
}

type BrokerEvent = { type: 'frame'; generation: number; frame: ScopeFrame }
  | { type: 'unavailable'; unavailable: ScopeUnavailable }
  | { type: 'invalidate'; generation: number }

type Subscriber = (event: BrokerEvent) => void

/** One renderer-owned source of exact scope taps. It never exposes display/CSS pixels. */
export class VideoScopeFrameBroker {
  private subscribers = new Set<Subscriber>()
  private generation = 0

  subscribe(subscriber: Subscriber): () => void {
    this.subscribers.add(subscriber)
    return () => this.subscribers.delete(subscriber)
  }

  invalidate(): number {
    const generation = ++this.generation
    this.emit({ type: 'invalidate', generation })
    return generation
  }

  publish(frame: ScopeFrame): void {
    if (frame.rgba.byteLength !== frame.identity.width * frame.identity.height * 4) {
      throw new Error('Scope frame RGBA byte length does not match its identity')
    }
    this.emit({ type: 'frame', generation: this.generation, frame })
  }

  unavailable(tapId: ScopeTapId, reason: string): void {
    this.emit({ type: 'unavailable', unavailable: { tapId, generation: this.generation, reason } })
  }

  currentGeneration(): number { return this.generation }

  private emit(event: BrokerEvent): void {
    for (const subscriber of this.subscribers) subscriber(event)
  }
}

export const videoScopeFrameBroker = new VideoScopeFrameBroker()
