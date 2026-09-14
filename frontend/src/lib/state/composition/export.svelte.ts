import * as api from '../../api.js'
import {
  authState
} from '../auth.svelte.js'
import {
  pollJob
} from '../../../data/jobs.js'
import {
  buildCompositionRenderRequest
} from '../../composition/payload.js'
import {
  COMPOSITION_DELIVERY_PROFILE_OPTIONS,
  compositionDeliveryProfileOption,
  type CompositionDeliveryProfileId,
  type CompositionRenderOutput
} from '../../composition/types.js'
import {
  compositionRenderUnavailableReason,
  compositionUsesFreezeFrame,
  compositionUsesOpticalFlow,
  compositionUsesReversePlayback,
  compositionUsesSpeedRamp,
  compositionUsesStabilization
} from '../../composition/validation.js'
import type {
  Capabilities,
  ResultInfo
} from '../../types.js'
import {
  commitDocument,
  compositionDuration,
  compositionRenderOutput,
  compositionState
} from './core.svelte.js'

export function updateCompositionExportSettings(
  patch: Partial<CompositionRenderOutput>,
): void {
  const profile = patch.profile ?? compositionState.export.profile
  const qualityTier = patch.qualityTier ?? compositionState.export.qualityTier
  const videoBitrateKbps = Object.hasOwn(patch, 'videoBitrateKbps')
    ? patch.videoBitrateKbps
    : compositionState.export.videoBitrateKbps ?? undefined
  compositionDeliveryProfileOption(profile)
  if (qualityTier !== 'high' && qualityTier !== 'medium' && qualityTier !== 'compact') {
    throw new Error('Неизвестный quality tier для composition export')
  }
  if (videoBitrateKbps !== undefined && (
    !Number.isSafeInteger(videoBitrateKbps) || videoBitrateKbps < 100 || videoBitrateKbps > 200_000 ||
    (profile.container !== 'mp4' && profile.container !== 'webm')
  )) {
    throw new Error('Custom video bitrate доступен для MP4/WebM в диапазоне 100..200000 Kbps')
  }
  commitDocument(compositionState.document, compositionState.media, {
    profile: { ...profile },
    qualityTier,
    ...(videoBitrateKbps === undefined ? {} : { videoBitrateKbps }),
  })
}


export function setCompositionDeliveryProfile(profileId: CompositionDeliveryProfileId): void {
  const option = COMPOSITION_DELIVERY_PROFILE_OPTIONS.find((candidate) => candidate.id === profileId)
  if (!option) throw new Error(`Неизвестный delivery profile ${profileId}`)
  updateCompositionExportSettings({
    profile: option.profile,
    ...(
      option.profile.container === 'mp4' || option.profile.container === 'webm'
        ? {}
        : { videoBitrateKbps: undefined }
    ),
  })
}


export function setCompositionExportRangePoint(point: 'in' | 'out'): void {
  const tick = compositionState.transport.playheadTicks
  if (point === 'in') compositionState.export.rangeInTicks = tick
  else compositionState.export.rangeOutTicks = tick
  compositionState.export.result = null
  compositionState.export.error = ''
}


export function compositionExportRange(): { startTicks: number; endTicks: number } | null {
  const startTicks = compositionState.export.rangeInTicks
  const endTicks = compositionState.export.rangeOutTicks
  return startTicks === null || endTicks === null ? null : { startTicks, endTicks }
}


export function getCompositionExportUnavailableReason(capabilities: Capabilities | null): string | null {
  if (!capabilities) return 'Проверяем поддержку composition-v1 на сервере…'
  const feature = capabilities.features?.find((candidate) => candidate.id === 'composition-v1')
  if (!feature) return 'Сервер не объявил поддержку composition-v1.'
  if (!feature.available) return feature.reason?.trim() || 'Composition export недоступен на этом сервере.'
  const renderReason = compositionRenderUnavailableReason(compositionState.document)
  if (renderReason) return renderReason
  const rangeIn = compositionState.export.rangeInTicks
  const rangeOut = compositionState.export.rangeOutTicks
  if ((rangeIn === null) !== (rangeOut === null)) return 'Задайте обе границы In и Out либо очистите диапазон.'
  if (rangeIn !== null && rangeOut !== null) {
    if (rangeIn >= rangeOut) return 'Граница In должна быть раньше Out.'
    if (rangeOut > compositionDuration()) return 'Граница Out выходит за длительность композиции.'
  }
  const delivery = compositionDeliveryProfileOption(compositionState.export.profile)
  if (delivery.capabilityId) {
    const deliveryCapability = capabilities.features?.find((candidate) => candidate.id === delivery.capabilityId)
    if (!deliveryCapability) {
      return `Сервер не объявил поддержку ${delivery.capabilityId} для ${delivery.label}.`
    }
    if (!deliveryCapability.available) {
      return deliveryCapability.reason?.trim() || `${delivery.label} недоступен на этом сервере.`
    }
  }
  if (compositionUsesOpticalFlow(compositionState.document)) {
    const opticalFlow = capabilities.features?.find((candidate) => candidate.id === 'optical-flow')
    if (!opticalFlow) return 'Сервер не объявил поддержку optical-flow для этого composition request.'
    if (!opticalFlow.available) return opticalFlow.reason?.trim() || 'Optical flow недоступен на этом сервере.'
  }
  if (compositionUsesReversePlayback(compositionState.document)) {
    const reversePlayback = capabilities.features?.find((candidate) => candidate.id === 'reverse-playback')
    if (!reversePlayback) return 'Сервер не объявил поддержку reverse-playback для этого composition request.'
    if (!reversePlayback.available) return reversePlayback.reason?.trim() || 'Reverse playback недоступен на этом сервере.'
  }
  if (compositionUsesFreezeFrame(compositionState.document)) {
    const freezeFrame = capabilities.features?.find((candidate) => candidate.id === 'freeze-frame')
    if (!freezeFrame) return 'Сервер не объявил поддержку freeze-frame для этого composition request.'
    if (!freezeFrame.available) return freezeFrame.reason?.trim() || 'Freeze frame недоступен на этом сервере.'
  }
  if (compositionUsesStabilization(compositionState.document)) {
    const stabilization = capabilities.features?.find((candidate) => candidate.id === 'stabilization')
    if (!stabilization) return 'Сервер не объявил поддержку stabilization для этого composition request.'
    if (!stabilization.available) return stabilization.reason?.trim() || 'Stabilization недоступна на этом сервере.'
  }
  if (compositionUsesSpeedRamp(compositionState.document)) {
    const speedRamp = capabilities.features?.find((candidate) => candidate.id === 'speed-ramp')
    if (!speedRamp) return 'Сервер не объявил поддержку speed-ramp для этого composition request.'
    if (!speedRamp.available) return speedRamp.reason?.trim() || 'Speed ramp недоступна на этом сервере.'
  }
  return null
}


export async function exportComposition(capabilities: Capabilities | null): Promise<void> {
  if (compositionState.export.running) return
  const reason = getCompositionExportUnavailableReason(capabilities)
  if (reason) {
    compositionState.export.error = reason
    return
  }
  compositionState.export.running = true
  compositionState.export.error = ''
  compositionState.export.result = null
  compositionState.export.progress = null
  compositionState.export.stage = 'queued'
  try {
    const range = compositionExportRange()
    const request = buildCompositionRenderRequest(compositionState.document, {
      ...compositionRenderOutput(),
      ...(range ? { range } : {}),
    })
    const { jobId } = await api.renderComposition(request, authState.token)
    compositionState.export.jobId = jobId
    const job = await pollJob(jobId, { onTick: (current) => {
      compositionState.export.progress = typeof current.progress === 'number' ? current.progress : null
      compositionState.export.stage = current.stage ?? null
    } })
    compositionState.export.result = job.result as ResultInfo
  } catch (error) {
    compositionState.export.error =
      error instanceof Error && error.message === 'cancelled'
        ? 'Экспорт отменён'
        : error instanceof Error
          ? error.message
          : String(error)
  } finally {
    compositionState.export.running = false
    compositionState.export.jobId = null
    compositionState.export.progress = null
    compositionState.export.stage = null
  }
}


export async function cancelCompositionExport(): Promise<void> {
  if (compositionState.export.jobId) await api.cancelJob(compositionState.export.jobId)
}

