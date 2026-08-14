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

export interface CapabilityOption {
  id: string
  label: string
  available: boolean
  reason?: string
}

export interface Capabilities {
  schemaVersion: number
  toolFingerprint: string
  formats: CapabilityOption[]
  codecs: CapabilityOption[]
  filters: CapabilityOption[]
  hardware: CapabilityOption[]
}

export interface ResultInfo {
  id: string
  url: string
  filename: string
  sizeBytes?: number | null
}

/** Metadata returned after the backend validates and stores a 3D `.cube` LUT. */
export interface LutAsset {
  id: string
  name: string
  cubeSize: number
  sizeBytes: number
  sha256?: string
}

export interface CurvePoint {
  /** Input code value, inclusive 0..255. */
  x: number
  /** Output code value, inclusive 0..255. */
  y: number
}

export interface ColorCurves {
  master: CurvePoint[]
  red: CurvePoint[]
  green: CurvePoint[]
  blue: CurvePoint[]
}

/** A source range kept by the single-source timeline. Array order is playback order. */
export interface TimelineSegment {
  id: string
  start: number
  end: number
}

export interface EditState {
  /** When enabled, timelineSegments is the canonical timing recipe. */
  timelineEnabled: boolean
  /** Ordered source ranges. Their array order is preserved in preview and export. */
  timelineSegments: TimelineSegment[]
  // Legacy timing fields stay persisted for backwards compatibility and migration.
  trimStart: number
  trimEnd: number
  cutEnabled: boolean
  cut: { start: number; end: number }
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
  normalizeAudio: boolean
  highpass: boolean
  brightness: number
  contrast: number
  saturation: number
  chromaKeyEnabled: boolean
  chromaKeyColor: string
  chromaKeySimilarity: number
  chromaKeyBlend: number
  chromaKeySpill: number
  filter: string
  /** Stored LUT asset reference. File bytes never enter edit/project JSON. */
  lutId: string | null
  lutName: string
  lutSize: number | null
  lutIntensity: number
  curves: ColorCurves
  reverse: boolean
  fps: number | null
  censorEnabled: boolean
  censor: { x: number; y: number; w: number; h: number }
  censorColor: string
  vignette: boolean
  denoise: boolean
  sharpen: number
  grain: number
  pad: string
  // round 3 export options
  format: string
  codec: string
  qualityTier: string
}

export interface MediaEntry {
  id: string
  kind: 'source' | 'output'
  filename: string
  url: string
  title?: string | null
  duration?: number | null
  width?: number | null
  height?: number | null
  sizeBytes?: number | null
  createdAt: number
}

export type JobStatus = 'pending' | 'running' | 'done' | 'error' | 'cancelled' | 'interrupted'

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
