import type { HslSelective } from './domain/hsl-selective'
import type { ColorManagementStatusV1 } from './domain/color-management'

export type { ColorManagementProvenanceV1, ColorManagementStatusV1, SdrColorDescriptorV1 } from './domain/color-management'

export type {
  HslAdjustment,
  HslSelection,
  HslSelective,
} from './domain/hsl-selective'

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
  mediaKind?: 'video' | 'audio'
  assetId?: string
  fingerprint?: string
  identityConflict?: boolean
  availability?: 'ready' | 'permission-required' | 'offline' | 'session'
  sizeBytes?: number | null
  colorManagement?: ColorManagementStatusV1 | null
}

export type {
  ProjectClip,
  ProjectDocument,
  ProjectEffect,
  ProjectEnvelope,
  ProjectMedia,
  ProjectSequence,
  ProjectTrack,
  SequenceSettings,
} from './project-schema'

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
  runtime?: {
    worker: boolean
    wasm: boolean
    workerFs: boolean
    opfs: boolean
    webCrypto: boolean
    streamingOutput: boolean
    memoryBudgetBytes: number
  }
}

export interface ResultInfo {
  id: string
  url: string
  filename: string
  sizeBytes?: number | null
}

/** Metadata returned after the backend validates and stores a 3D `.cube` LUT. */
export interface LutAsset {
  schemaVersion?: number
  id: string
  name: string
  kind?: 'cube3d'
  cubeSize: number
  sizeBytes: number
  sha256?: string
  createdAt?: number
  favorite?: boolean
}

export interface LutBakeRequest {
  edit: {
    brightness?: number
    contrast?: number
    saturation?: number
    filter?: 'grayscale' | 'sepia'
    curves?: ColorCurves
  }
  size: 33
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

export interface ColorWheelChannels {
  master: number
  red: number
  green: number
  blue: number
}

export interface EditState {
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
  /** Primary corrections; neutral 0, canonical range -1..1. */
  temperature: number
  tint: number
  highlights: number
  shadows: number
  /** Lift/Gamma/Gain controls in linear sRGB; every channel is neutral at 0. */
  lift: ColorWheelChannels
  gamma: ColorWheelChannels
  gain: ColorWheelChannels
  /** Encoded-sRGB selective HSL range and adjustment. */
  hslSelective: HslSelective
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
  fps?: number | null
  vcodec?: string | null
  acodec?: string | null
  mediaKind?: 'video' | 'audio'
  assetId?: string
  availability?: 'ready' | 'permission-required' | 'offline' | 'session'
  fingerprint?: string
  identityConflict?: boolean
  sizeBytes?: number | null
  colorManagement?: ColorManagementStatusV1 | null
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
