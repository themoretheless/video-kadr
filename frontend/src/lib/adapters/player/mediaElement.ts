import {
  clampPlaybackRate,
  clampPlayerTime,
  clampPlayerVolume,
  type PlayerAdapter,
  type PlayerEventName,
  type PlayerSnapshot,
} from '$lib/ports/player.js'

export class MediaElementPlayerAdapter implements PlayerAdapter {
  constructor(private readonly element: HTMLMediaElement) {}

  snapshot(): PlayerSnapshot {
    return {
      currentTime: this.element.currentTime,
      duration: this.element.duration,
      muted: this.element.muted,
      paused: this.element.paused,
      playbackRate: this.element.playbackRate,
      volume: this.element.volume,
    }
  }

  async play(): Promise<void> {
    await this.element.play()
  }

  pause(): void {
    this.element.pause()
  }

  seek(seconds: number): void {
    this.element.currentTime = clampPlayerTime(seconds, this.element.duration)
  }

  setMuted(muted: boolean): void {
    this.element.muted = muted
  }

  setPlaybackRate(rate: number): void {
    this.element.playbackRate = clampPlaybackRate(rate)
  }

  setVolume(volume: number): void {
    this.element.volume = clampPlayerVolume(volume)
  }

  load(): void {
    this.element.load()
  }

  subscribe(event: PlayerEventName, listener: () => void): () => void {
    this.element.addEventListener(event, listener)
    return () => this.element.removeEventListener(event, listener)
  }
}
