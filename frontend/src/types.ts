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
