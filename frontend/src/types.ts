export interface VideoInfo {
  id: string
  url: string
  filename: string
  duration: number
  width: number
  height: number
  title?: string | null
  fps?: number | null
  vcodec?: string | null
  acodec?: string | null
  sizeBytes?: number | null
}

export interface ResultInfo {
  id: string
  url: string
  filename: string
  sizeBytes?: number | null
}

export interface EditState {
  trimStart: number
  trimEnd: number
  cropEnabled: boolean
  crop: { x: number; y: number; w: number; h: number }
  scaleEnabled: boolean
  scale: { w: number; h: number }
  mute: boolean
  speed: number
  // round 2 effects
  rotate: number
  flipH: boolean
  flipV: boolean
  volume: number
  fadeIn: number
  fadeOut: number
  brightness: number
  contrast: number
  saturation: number
  filter: string
  reverse: boolean
  fps: number | null
  // round 3 export options
  format: string
  codec: string
  qualityTier: string
}

export type JobStatus = 'pending' | 'running' | 'done' | 'error' | 'cancelled'

export interface Job {
  id: string
  status: JobStatus
  result?: unknown
  error?: string
  /** 0..100, omitted by the backend when unknown. */
  progress?: number
  /** One of "queued" | "downloading" | "processing". */
  stage?: string
}
