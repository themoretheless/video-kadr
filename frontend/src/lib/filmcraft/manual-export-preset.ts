import type { FilmcraftExportPreset, FilmcraftVideoRequest } from '$lib/api'

const builtins: Partial<Record<FilmcraftExportPreset, string>> = {
  adaptive_high: 'Match Source – Adaptive High Bitrate',
  adaptive_medium: 'Match Source – Adaptive Medium Bitrate',
  adaptive_low: 'Match Source – Adaptive Low Bitrate',
  high_quality1080: 'High Quality 1080p HD',
  high_quality2160: 'High Quality 2160p 4K',
  youtube1080: 'YouTube 1080p Full HD',
  youtube2160: 'YouTube 2160p 4K Ultra HD',
  vimeo1080: 'Vimeo 1080p Full HD',
  vimeo2160: 'Vimeo 2160p 4K Ultra HD',
  social_vertical: 'Social Vertical 1080×1920',
  prores_hq: 'Apple ProRes 422 HQ',
  prores_standard: 'Apple ProRes 422',
  prores_lt: 'Apple ProRes 422 LT',
  prores_proxy: 'Apple ProRes 422 Proxy',
  dnxhr_hq: 'Avid DNxHR HQ',
  dnxhr_sq: 'Avid DNxHR SQ',
  dnxhr_lb: 'Avid DNxHR LB',
  apv42210: 'APV 422-10',
  apv42212: 'APV 422-12',
  mxf_dnxhr_hq: 'MXF OP1a DNxHR HQ',
  mxf_prores_hq: 'MXF OP1a ProRes 422 HQ',
  mxf_h264: 'MXF OP1a H.264',
  mxf_atom_dnxhr_hq: 'MXF OP-Atom DNxHR (Avid)',
  png_sequence: 'PNG Sequence',
  tiff_sequence: 'TIFF Sequence',
  bmp_sequence: 'BMP Sequence',
  gif: 'Animated GIF 640×360',
  wav16: 'Waveform Audio 48 kHz 16-bit',
  wav24: 'Waveform Audio 48 kHz 24-bit',
  aiff16: 'AIFF 48 kHz 16-bit',
}

export function manualExportPreset(request: FilmcraftVideoRequest) {
  const preset = request.preset
  const from = preset && builtins[preset]
  if (request.libraryPreset || (!from && preset !== 'hevc' && preset !== 'mjpeg')) return null

  const {
    frameSize, frameRate, pixelAspect, keyframeDistance, h264Level, h264Profile, scaling,
    scale, maxRenderQuality, captionSidecar, nameOverlay, timecodeOverlay, imageOverlay, loudness,
    videoLimiter, metadata, bitrate, mjpegQuality, multiplexer,
  } = request
  const audio = request.audio ?? request.h264Audio
  const settings = {
    ...(!from ? { format: preset } : {}),
    ...(preset === 'hevc' && !bitrate ? { bitrateKbps: 10000 } : {}),
    includeAudio: request.withAudio ?? false,
    burnCaptions: request.burnCaptions ?? true,
    captionSidecar: captionSidecar ?? null,
    ...(frameSize ? { frameSize } : {}),
    ...(scale !== undefined ? { scale } : {}),
    ...(frameRate ? { frameRate: { num: frameRate.numerator, den: frameRate.denominator } } : {}),
    ...(pixelAspect ? { pixelAspect } : {}),
    ...(keyframeDistance ? { keyframeDistance } : {}),
    ...(h264Level !== undefined ? { h264Level } : {}),
    ...(h264Profile ? { h264Profile } : {}),
    ...(scaling ? { scaling } : {}),
    ...(maxRenderQuality ? { maxRenderQuality } : {}),
    ...(mjpegQuality ? { quality: mjpegQuality } : {}),
    ...(multiplexer ? { multiplexer } : {}),
    ...(metadata ? { metadata } : {}),
    ...(bitrate ? {
      bitrateMode: bitrate.mode, bitrateKbps: bitrate.targetKbps,
      adaptiveBitrate: bitrate.adaptiveBpp ?? null, maxBitrateKbps: bitrate.maxKbps ?? null,
    } : {}),
    audio: {
      codec: audio?.codec ?? (preset === 'mjpeg' ? 'pcm' : 'auto'),
      bits: audio?.bits ?? (preset === 'mjpeg' || preset === 'wav24' ? 24 : undefined),
      channels: audio?.channels ?? 2,
      sample_rate: audio?.sampleRate === 0 ? null : (audio?.sampleRate ?? 48000),
      bitrate_kbps: audio?.bitrateKbps ?? 320,
    },
    effects: {
      ...(nameOverlay ? { name_overlay: portableTextOverlay(nameOverlay) } : {}),
      ...(timecodeOverlay ? { timecode_overlay: portableTextOverlay(timecodeOverlay) } : {}),
      ...(imageOverlay ? { image_overlay: {
        enabled: imageOverlay.enabled, source_id: imageOverlay.sourceId, placement: imageOverlay.placement,
        offset: imageOverlay.offset, size_percent: imageOverlay.sizePercent, opacity: imageOverlay.opacity,
      } } : {}),
      ...(loudness ? { loudness: { enabled: loudness.enabled, target_lufs: loudness.targetLufs, true_peak_dbtp: loudness.truePeakDbtp } } : {}),
      ...(videoLimiter ? { video_limiter: { enabled: videoLimiter.enabled, min_percent: videoLimiter.minPercent, max_percent: videoLimiter.maxPercent } } : {}),
    },
  }
  return { from, settings }
}

function portableTextOverlay(overlay: NonNullable<FilmcraftVideoRequest['nameOverlay']>) {
  const { sizePercent, ...rest } = overlay
  return { ...rest, size_percent: sizePercent }
}
