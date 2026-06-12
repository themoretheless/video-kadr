export interface VideoInfo {
  id: string
  url: string
  filename: string
  duration: number
  width: number
  height: number
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

export type JobStatus = 'pending' | 'running' | 'done' | 'error'

export interface Job {
  id: string
  status: JobStatus
  result?: unknown
  error?: string
}
