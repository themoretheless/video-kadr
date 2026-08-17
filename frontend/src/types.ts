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

// --- shared primitives for the composition/overlay/motion feature wave ---

/** Interpolation applied between a keyframe and the next one. */
export type Interpolation = 'hold' | 'linear' | 'smooth'

/** A single keyframe on the OUTPUT timeline (`t` in seconds). */
export interface Keyframe {
  t: number
  v: number
  interp: Interpolation
}

/** Keyframes sorted by `t`, at most 64 points. Empty means "parameter unused". */
export type KeyframeTrack = Keyframe[]

/** FFmpeg `xfade` transition ids; audio uses `acrossfade` with the same duration. */
export type TransitionKind =
  | 'fade'
  | 'wipeleft'
  | 'wiperight'
  | 'wipeup'
  | 'wipedown'
  | 'slideleft'
  | 'slideright'
  | 'slideup'
  | 'slidedown'
  | 'circleopen'
  | 'circleclose'
  | 'dissolve'
  | 'pixelize'
  | 'radial'
  | 'smoothleft'
  | 'smoothright'
  | 'zoomin'

export interface Transition {
  kind: TransitionKind
  /** Seconds of overlap taken from both neighbours, 0.05..3.0. */
  duration: number
}

/** One entry of a multi-source timeline; replaces videoId+segments when present. */
export interface ClipSpec {
  sourceId: string
  /** In-point and out-point inside the SOURCE, seconds. */
  start: number
  end: number
  speed?: number
  volume?: number
  muted?: boolean
  /** Always null on the first clip: there is nothing to blend from. */
  transitionIn?: Transition | null
}

export type OverlayKind = 'image' | 'video'

export interface ChromaKeySpec {
  color: string
  similarity: number
  blend: number
}

export interface OverlayAudioSpec {
  enabled: boolean
  volume: number
}

/** Watermark, logo, picture-in-picture or green-screen layer. */
export interface OverlaySpec {
  assetId: string
  kind: OverlayKind
  /** Normalized top-left corner, 0..1 of the output frame. */
  x: number
  y: number
  /** Normalized size; a null height keeps the source aspect ratio. */
  width: number
  height?: number | null
  opacity?: number
  rotation?: number
  /** Output-timeline seconds; a null end runs to the end of the render. */
  start?: number
  end?: number | null
  fadeIn?: number
  fadeOut?: number
  chromaKey?: ChromaKeySpec | null
  /** Only meaningful for `kind: "video"`. */
  audio?: OverlayAudioSpec | null
}

export type TitleAlign = 'center' | 'left' | 'right'
export type TitleAnimation = 'none' | 'fade' | 'slide-up' | 'typewriter' | 'pop'

export interface TitleBoxSpec {
  color: string
  opacity: number
  padding: number
}

/** A drawtext title or lower third. */
export interface TitleSpec {
  text: string
  /** Null uses the bundled default font. */
  fontAssetId?: string | null
  /** Pixels at 1080p, scaled to the real output height. */
  fontSize: number
  color: string
  /** Anchor point, normalized 0..1 of the output frame. */
  x: number
  y: number
  align: TitleAlign
  box?: TitleBoxSpec | null
  borderWidth?: number
  borderColor?: string
  shadowX?: number
  shadowY?: number
  shadowColor?: string
  start?: number
  end?: number | null
  fadeIn?: number
  fadeOut?: number
  animation?: TitleAnimation
}

export type SubtitlePosition = 'bottom' | 'top'

/** A stored `.srt` / `.vtt` asset burned into (or muxed alongside) the output. */
export interface SubtitleSpec {
  assetId: string
  burnIn: boolean
  fontSize: number
  color: string
  outlineWidth: number
  position: SubtitlePosition
  marginV: number
}

export type AudioRole = 'music' | 'voiceover' | 'sfx'

export interface DuckingSpec {
  enabled: boolean
  threshold: number
  ratio: number
  /** Milliseconds. */
  attack: number
  release: number
}

/** An extra audio asset mixed onto the output timeline. */
export interface AudioTrackSpec {
  assetId: string
  role: AudioRole
  gain?: number
  /** Where the track lands on the OUTPUT timeline, seconds. */
  start?: number
  /** In-point inside the asset, seconds. */
  sourceStart?: number
  end?: number | null
  loop?: boolean
  fadeIn?: number
  fadeOut?: number
  ducking?: DuckingSpec | null
}

export interface CompressorSpec {
  /** dBFS. */
  threshold: number
  ratio: number
  /** Milliseconds. */
  attack: number
  release: number
  makeup: number
}

export interface LimiterSpec {
  /** dBFS ceiling. */
  ceiling: number
}

export interface GateSpec {
  /** dBFS. */
  threshold: number
  ratio: number
}

export interface AudioDynamics {
  /** 0..1, mapped onto the `afftdn` noise reduction amount. */
  denoise?: number
  dereverb?: boolean
  compressor?: CompressorSpec | null
  limiter?: LimiterSpec | null
  gate?: GateSpec | null
  deesser?: boolean
  /** Overrides the legacy `highpass` boolean when set. */
  highpassHz?: number | null
  lowpassHz?: number | null
  bitrateKbps?: number
  /** Multiplies the static `volume` over time. */
  volumeEnvelope?: KeyframeTrack
}

/** Keyframed transform: Ken Burns moves and animated reframing. */
export interface MotionSpec {
  /** 1.0 = fit, above 1 punches in. */
  zoom?: KeyframeTrack
  /** -1..1 of the frame. */
  panX?: KeyframeTrack
  panY?: KeyframeTrack
  /** Degrees. */
  rotation?: KeyframeTrack
}

export type InputProjection = 'equirect' | 'fisheye' | 'dfisheye'
export type OutputProjection = 'flat' | 'equirect' | 'fisheye' | 'stereographic' | 'pannini'

/** Insta360-style reframing of a 360/action-cam source. */
export interface Reframe360Spec {
  inputProjection: InputProjection
  outputProjection: OutputProjection
  /** All four tracks are in degrees. */
  fov?: KeyframeTrack
  yaw?: KeyframeTrack
  pitch?: KeyframeTrack
  roll?: KeyframeTrack
  outputWidth: number
  outputHeight: number
  horizonLock: boolean
}

export type StabilizeMode = 'off' | 'fast' | 'precise'

export interface StabilizeSpec {
  /** `fast` uses deshake, `precise` runs the two-pass vidstab chain. */
  mode: StabilizeMode
  /** 1..100. */
  smoothing: number
  /** Crop-in percentage, 0..20. */
  zoom: number
  horizonLock: boolean
}

export interface LensCorrection {
  k1: number
  k2: number
}

/** Colour wheel offsets (lift) or multipliers (gamma, gain). */
export interface Rgb {
  r: number
  g: number
  b: number
}

export type HslBand = 'red' | 'orange' | 'yellow' | 'green' | 'cyan' | 'blue' | 'magenta'

export interface HslAdjustment {
  band: HslBand
  hue: number
  saturation: number
  luminance: number
}

/** Resolve-lite primary grade applied before the look preset, curves and LUT. */
export interface ColorAdvanced {
  /** -1..1. */
  temperature?: number
  tint?: number
  /** Stops, -2..2. */
  exposure?: number
  /** -1..1. */
  highlights?: number
  shadows?: number
  /** -0.5..0.5 per channel. */
  lift?: Rgb
  /** 0.1..4 per channel. */
  gamma?: Rgb
  /** 0..4 per channel. */
  gain?: Rgb
  hsl?: HslAdjustment[]
}

export type AssetKind = 'image' | 'audio' | 'video' | 'font' | 'subtitle'

/** Metadata for a stored asset. Renders reference assets by id, never by path. */
export interface AssetEntry {
  id: string
  kind: AssetKind
  filename: string
  mime: string
  sizeBytes: number
  sha256: string
  width?: number | null
  height?: number | null
  duration?: number | null
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
