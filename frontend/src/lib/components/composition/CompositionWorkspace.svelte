<script lang="ts">
  import NativeTemplateDownload from './NativeTemplateDownload.svelte'
  import NativeTemplateImport from './NativeTemplateImport.svelte'
  import NativeTemplateLibrary from './NativeTemplateLibrary.svelte'
  import NestedConcatTreatmentInsert from "./NestedConcatTreatmentInsert.svelte"
  import NestedConcatTreatmentInspector from "./NestedConcatTreatmentInspector.svelte"
  import NestedConcatShapeInsert from "./NestedConcatShapeInsert.svelte"
  import NestedConcatShapeInspector from "./NestedConcatShapeInspector.svelte"
  import NestedConcatTitleInsert from "./NestedConcatTitleInsert.svelte"
  import NestedConcatTitleInspector from "./NestedConcatTitleInspector.svelte"
  import {agentSession} from '$lib/agent/session.svelte'
  import NestedCaptionExport from './NestedCaptionExport.svelte'
  import CaptionStylePanel from './CaptionStylePanel.svelte'
  import NestedDetachedAudioInspector from "./NestedDetachedAudioInspector.svelte"
  import NestedAudioStreamInspector from "./NestedAudioStreamInspector.svelte"
  import NestedConcatGainInspector from "./NestedConcatGainInspector.svelte"
  import NestedMotionPathOverlay from "./NestedMotionPathOverlay.svelte"
  import NestedMotionPathInspector from "./NestedMotionPathInspector.svelte"
  import NestedScalarTimeline from "./NestedScalarTimeline.svelte"
  import NestedScalarKeyInspector from "./NestedScalarKeyInspector.svelte"
  import NestedEmbeddedAudioInspector from "./NestedEmbeddedAudioInspector.svelte"
  import ConcatProjectImportPanel from "./ConcatProjectImportPanel.svelte"
  import ConcatProjectExportPanel from "./ConcatProjectExportPanel.svelte"
  import {compositionSourceFromLibraryEntry} from "$lib/state/composition.svelte"
  import NestedPlaybackControls from './NestedPlaybackControls.svelte'
  import { importPreferences, setAutoPairAudioOnImport } from '$lib/state/importPreferences.svelte.js'
  import {
    compositionProjectArchiveUrl,
    importCompositionProjectArchive,
    MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES,
    subscribeCompositionProjectChanges,
  } from '$lib/api.js'
  import type { ReviewThreadDto } from '$lib/api.js'
  import CapturePanel from '$lib/capture/CapturePanel.svelte'
  import type { CaptureRuntime } from '$lib/capture/types.js'
  import VoiceoverPanel from '$lib/audio/VoiceoverPanel.svelte'
  import type { VoiceoverRuntime } from '$lib/audio/types.js'
  import { voiceoverFileValidationError } from '$lib/composition/voiceover.js'
  import type { MediaInfo } from '$lib/types.js'
  import {
    compositionState,
    addMediaInfoToComposition,
    addVoiceoverMediaInfoToComposition,
    deleteCompositionProject,
    loadCompositionProjects,
    newComposition,
    openCompositionProject,
    refreshOpenCompositionProject,
    refreshCompositionProjects,
    saveCompositionProject,
    setCompositionProjectName,
    syncCompositionLibrary,
  } from '$lib/state/composition.svelte.js'
  import { doUpload, loadLibrary, state as legacyState } from '$lib/state/store.svelte.js'
  import { authState, logout } from '$lib/state/auth.svelte.js'
  import { spacesState } from '$lib/state/spaces.svelte.js'
  import AuthPanel from '$lib/components/review/AuthPanel.svelte'
  import SpacePanel from '$lib/components/collaboration/SpacePanel.svelte'
  import LazyShotReview from '$lib/analysis/LazyShotReview.svelte'
  import CompositionLinks from './CompositionLinks.svelte'
  import CompositionSearch from '$lib/search/CompositionSearch.svelte'
  import AgentPanel from '$lib/agent/AgentPanel.svelte'
  import CompositionLocalTools from './CompositionLocalTools.svelte'
  import CompositionInspector from './CompositionInspector.svelte'
  import CompositionPreview from './CompositionPreview.svelte'
  import NativeSpeechPanel from './NativeSpeechPanel.svelte'
  import NestedClipTransfer from './NestedClipTransfer.svelte'
  import OrdinaryHistoryPanel from './OrdinaryHistoryPanel.svelte'
  import NestedHistoryPanel from './NestedHistoryPanel.svelte'
  import NestedMarkers from './NestedMarkers.svelte'
  import NestedTrackControls from './NestedTrackControls.svelte'
  import NestedMediaInsert from './NestedMediaInsert.svelte'
  import NestedProjectMedia from './NestedProjectMedia.svelte'
  import NestedTimelineControls from './NestedTimelineControls.svelte'
  import NestedCompositionPreview from './NestedCompositionPreview.svelte'
  import NestedFrameExport from './NestedFrameExport.svelte'
  import NestedScopes from './NestedScopes.svelte'
  import NestedExportPanel from './NestedExportPanel.svelte'
  import FilmcraftProjectPanel from './FilmcraftProjectPanel.svelte'
  import NestedChromaInspector from './NestedChromaInspector.svelte'
  import NestedMaskInspector from './NestedMaskInspector.svelte'
  import ProjectFonts from './ProjectFonts.svelte'
  import NestedEffectStack from './NestedEffectStack.svelte'
  import NestedHslInspector from './NestedHslInspector.svelte'
import NestedVideoLimiterInspector from './NestedVideoLimiterInspector.svelte'
import NestedCineonInspector from './NestedCineonInspector.svelte'
import NestedDirectionalBlurInspector from './NestedDirectionalBlurInspector.svelte'
import NestedDirectionalBlurLegacyInspector from './NestedDirectionalBlurLegacyInspector.svelte'
import NestedNoiseInspector from './NestedNoiseInspector.svelte'
import NestedNoiseLegacyInspector from './NestedNoiseLegacyInspector.svelte'
import NestedStrobeInspector from './NestedStrobeInspector.svelte'
import NestedReplicateInspector from './NestedReplicateInspector.svelte'
import NestedBrushStrokesInspector from './NestedBrushStrokesInspector.svelte'
import NestedRoughenEdgesInspector from './NestedRoughenEdgesInspector.svelte'
import NestedLongShadowInspector from './NestedLongShadowInspector.svelte'
import NestedAlphaGlowInspector from './NestedAlphaGlowInspector.svelte'
import NestedBlockDissolveInspector from './NestedBlockDissolveInspector.svelte'
import NestedGradientWipeInspector from './NestedGradientWipeInspector.svelte'
import NestedLinearWipeInspector from './NestedLinearWipeInspector.svelte'
import NestedMagnifyInspector from './NestedMagnifyInspector.svelte'
import NestedOffsetInspector from './NestedOffsetInspector.svelte'
import NestedVolumetricRaysInspector from './NestedVolumetricRaysInspector.svelte'
import NestedVrChromaticInspector from './NestedVrChromaticInspector.svelte'
import NestedVrPlaneToSphereInspector from './NestedVrPlaneToSphereInspector.svelte'
import NestedLightningInspector from './NestedLightningInspector.svelte'
import NestedWriteOnInspector from './NestedWriteOnInspector.svelte'
import NestedTimecodeInspector from './NestedTimecodeInspector.svelte'
import NestedClipNameInspector from './NestedClipNameInspector.svelte'
import NestedSimpleTextInspector from './NestedSimpleTextInspector.svelte'
import NestedMetadataBurninInspector from './NestedMetadataBurninInspector.svelte'
import NestedVrGlowInspector from './NestedVrGlowInspector.svelte'
import NestedVrFractalNoiseInspector from './NestedVrFractalNoiseInspector.svelte'
import NestedVrDigitalGlitchInspector from './NestedVrDigitalGlitchInspector.svelte'
import NestedVrColorGradientsInspector from './NestedVrColorGradientsInspector.svelte'
import NestedVrDenoiseInspector from './NestedVrDenoiseInspector.svelte'
import NestedVrBlurInspector from './NestedVrBlurInspector.svelte'
import NestedVrSharpenInspector from './NestedVrSharpenInspector.svelte'
import NestedVrProjectionInspector from './NestedVrProjectionInspector.svelte'
import NestedSphericalVideoInspector from './NestedSphericalVideoInspector.svelte'
import NestedVrRotateSphereInspector from './NestedVrRotateSphereInspector.svelte'
import NestedLightLeaksInspector from './NestedLightLeaksInspector.svelte'
  import NestedWarpStabilizerInspector from './NestedWarpStabilizerInspector.svelte'
  import NestedAutoReframeInspector from './NestedAutoReframeInspector.svelte'
  import NestedEchoInspector from './NestedEchoInspector.svelte'
  import NestedPosterizeTimeInspector from './NestedPosterizeTimeInspector.svelte'
import NestedGlintInspector from './NestedGlintInspector.svelte'
import NestedWonderGlowInspector from './NestedWonderGlowInspector.svelte'
import NestedEchoGlowInspector from './NestedEchoGlowInspector.svelte'
import NestedEdgeGlowInspector from './NestedEdgeGlowInspector.svelte'
import NestedRgbSplitInspector from './NestedRgbSplitInspector.svelte'
import NestedMirrorInspector from './NestedMirrorInspector.svelte'
import NestedSpherizeInspector from './NestedSpherizeInspector.svelte'
import NestedCornerPinInspector from './NestedCornerPinInspector.svelte'
import NestedRotate3DInspector from './NestedRotate3DInspector.svelte'
import NestedGrowInspector from './NestedGrowInspector.svelte'
import NestedSpinInspector from './NestedSpinInspector.svelte'
import NestedMoveInspector from './NestedMoveInspector.svelte'
import NestedWiggleInspector from './NestedWiggleInspector.svelte'
import NestedCameraShakeInspector from './NestedCameraShakeInspector.svelte'
import NestedSpacerInspector from './NestedSpacerInspector.svelte'
import NestedCompoundBlurInspector from './NestedCompoundBlurInspector.svelte'
import NestedTrackMatteInspector from './NestedTrackMatteInspector.svelte'
import NestedGraphicTextInspector from './NestedGraphicTextInspector.svelte'
import NestedGraphicShapeInspector from './NestedGraphicShapeInspector.svelte'
import NestedBokehBlurInspector from './NestedBokehBlurInspector.svelte'
import NestedUnsharpMaskInspector from './NestedUnsharpMaskInspector.svelte'
import NestedCropInspector from './NestedCropInspector.svelte'
import NestedTransformEffectInspector from './NestedTransformEffectInspector.svelte'
import NestedStandaloneVignetteInspector from './NestedStandaloneVignetteInspector.svelte'
import NestedLogoCutoutInspector from './NestedLogoCutoutInspector.svelte'
import NestedLightingEffectsInspector from './NestedLightingEffectsInspector.svelte'
import NestedAscCdlInspector from './NestedAscCdlInspector.svelte'
import NestedBrightnessContrastInspector from './NestedBrightnessContrastInspector.svelte'
import NestedFocusBlurInspector from './NestedFocusBlurInspector.svelte'
import NestedTurbulentDisplaceInspector from './NestedTurbulentDisplaceInspector.svelte'
import NestedChannelBlurInspector from './NestedChannelBlurInspector.svelte'
import NestedReduceInterlaceFlickerInspector from './NestedReduceInterlaceFlickerInspector.svelte'
import NestedStandaloneSharpenInspector from './NestedStandaloneSharpenInspector.svelte'
import NestedEdgeFeatherInspector from './NestedEdgeFeatherInspector.svelte'
import NestedAutoAlignInspector from './NestedAutoAlignInspector.svelte'
import NestedCloneInspector from './NestedCloneInspector.svelte'
import NestedRoundedCropInspector from './NestedRoundedCropInspector.svelte'
import NestedShrinkInspector from './NestedShrinkInspector.svelte'
import NestedStrokeInspector from './NestedStrokeInspector.svelte'
import NestedColorEmbossInspector from './NestedColorEmbossInspector.svelte'
import NestedGradientInspector from './NestedGradientInspector.svelte'
import NestedPaintBucketInspector from './NestedPaintBucketInspector.svelte'
import NestedEllipseInspector from './NestedEllipseInspector.svelte'
import NestedCellPatternInspector from './NestedCellPatternInspector.svelte'
import NestedCheckerboardInspector from './NestedCheckerboardInspector.svelte'
import NestedLensFlareInspector from './NestedLensFlareInspector.svelte'
import NestedGridInspector from './NestedGridInspector.svelte'
import NestedCircleInspector from './NestedCircleInspector.svelte'
import NestedRampInspector from './NestedRampInspector.svelte'
import NestedFourColorGradientInspector from './NestedFourColorGradientInspector.svelte'
import NestedBevelAlphaInspector from './NestedBevelAlphaInspector.svelte'
import NestedDropShadowInspector from './NestedDropShadowInspector.svelte'
import NestedBasic3DInspector from './NestedBasic3DInspector.svelte'
import NestedLensDistortionInspector from './NestedLensDistortionInspector.svelte'
import NestedWaveWarpInspector from './NestedWaveWarpInspector.svelte'
import NestedTwirlInspector from './NestedTwirlInspector.svelte'
import NestedTwirlLegacyInspector from './NestedTwirlLegacyInspector.svelte'
import NestedEmbossInspector from './NestedEmbossInspector.svelte'
import NestedFindEdgesInspector from './NestedFindEdgesInspector.svelte'
import NestedMosaicInspector from './NestedMosaicInspector.svelte'
import NestedMosaicLegacyInspector from './NestedMosaicLegacyInspector.svelte'
import NestedMedianInspector from './NestedMedianInspector.svelte'
import NestedCameraBlurInspector from './NestedCameraBlurInspector.svelte'
import NestedGaussianBlurInspector from './NestedGaussianBlurInspector.svelte'
import NestedGaussianBlurLegacyInspector from './NestedGaussianBlurLegacyInspector.svelte'
import NestedGammaInspector from './NestedGammaInspector.svelte'
import NestedPosterizeInspector from './NestedPosterizeInspector.svelte'
import NestedInvertInspector from './NestedInvertInspector.svelte'
import NestedLevelsInspector from './NestedLevelsInspector.svelte'
import NestedExtractInspector from './NestedExtractInspector.svelte'
import NestedBlackWhiteInspector from './NestedBlackWhiteInspector.svelte'
import NestedHorizontalFlipInspector from './NestedHorizontalFlipInspector.svelte'
import NestedVerticalFlipInspector from './NestedVerticalFlipInspector.svelte'
import NestedTintInspector from './NestedTintInspector.svelte'
import NestedColorBalanceInspector from './NestedColorBalanceInspector.svelte'
import NestedLeaveColorInspector from './NestedLeaveColorInspector.svelte'
import NestedChangeToColorInspector from './NestedChangeToColorInspector.svelte'
import NestedUltraKeyInspector from './NestedUltraKeyInspector.svelte'
import NestedColorKeyInspector from './NestedColorKeyInspector.svelte'
import NestedLumaKeyInspector from './NestedLumaKeyInspector.svelte'
import NestedAlphaAdjustInspector from './NestedAlphaAdjustInspector.svelte'
import NestedChannelMixInspector from './NestedChannelMixInspector.svelte'
import NestedColorReplaceInspector from './NestedColorReplaceInspector.svelte'
import NestedColorPassInspector from './NestedColorPassInspector.svelte'
import NestedProcAmpInspector from './NestedProcAmpInspector.svelte'
import NestedVignetteInspector from './NestedVignetteInspector.svelte'
import NestedLumetriInspector from './NestedLumetriInspector.svelte'
import NestedBasicGradeInspector from './NestedBasicGradeInspector.svelte'
import NestedCreativeAdjustInspector from './NestedCreativeAdjustInspector.svelte'
import NestedSharpenInspector from './NestedSharpenInspector.svelte'
 import NestedConcatContrastInspector from './NestedConcatContrastInspector.svelte'
 import NestedConcatExposureInspector from './NestedConcatExposureInspector.svelte'
 import NestedConcatChromaKeyInspector from './NestedConcatChromaKeyInspector.svelte'
 import NestedConcatLumaKeyInspector from './NestedConcatLumaKeyInspector.svelte'
 import NestedConcatPixelateInspector from './NestedConcatPixelateInspector.svelte'
 import NestedConcatGaussianBlurInspector from './NestedConcatGaussianBlurInspector.svelte'
 import NestedConcatMotionBlurInspector from './NestedConcatMotionBlurInspector.svelte'
 import NestedConcatBloomPulseInspector from './NestedConcatBloomPulseInspector.svelte'
 import NestedConcatZoomBlurInspector from './NestedConcatZoomBlurInspector.svelte'
 import NestedConcatFilmGrainInspector from './NestedConcatFilmGrainInspector.svelte'
 import NestedConcatGlowInspector from './NestedConcatGlowInspector.svelte'
 import NestedConcatHalftoneInspector from './NestedConcatHalftoneInspector.svelte'
 import NestedConcatFisheyeInspector from './NestedConcatFisheyeInspector.svelte'
 import NestedConcatChromaticAberrationInspector from './NestedConcatChromaticAberrationInspector.svelte'
 import NestedConcatPosterizeInspector from './NestedConcatPosterizeInspector.svelte'
 import NestedConcatSaturationInspector from './NestedConcatSaturationInspector.svelte'
 import NestedConcatSpatialInspector from './NestedConcatSpatialInspector.svelte'
 import NestedConcatAdjustInspector from './NestedConcatAdjustInspector.svelte'
  import NestedConcatCurvesInspector from './NestedConcatCurvesInspector.svelte'
 import NestedConcatColorWheelsInspector from './NestedConcatColorWheelsInspector.svelte'
 import NestedConcatWhiteBalanceInspector from './NestedConcatWhiteBalanceInspector.svelte'
 import NestedConcatVibranceInspector from './NestedConcatVibranceInspector.svelte'
  import NestedLutInspector from './NestedLutInspector.svelte'
  import NestedCreativeGradeInspector from './NestedCreativeGradeInspector.svelte'
  import NestedToneCurvesInspector from './NestedToneCurvesInspector.svelte'
  import { nestedPlayback, nestedComparison, nestedComposition, closeNestedComposition, nestedHistory, groupNestedDocument, splitNestedInstance, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
  import CompositionReviewPanel from './CompositionReviewPanel.svelte'
  import MultiTrackTimeline from './MultiTrackTimeline.svelte'
  import YouTubePublishPanel from './YouTubePublishPanel.svelte'

  interface Props {
    captureRuntime?: CaptureRuntime
    captureCountdownSeconds?: number
    captureUploader?: (file: File, openLegacy?: boolean) => Promise<MediaInfo | null>
    voiceoverRuntime?: VoiceoverRuntime
    voiceoverCountdownSeconds?: number
    voiceoverUploader?: (file: File, openLegacy?: boolean) => Promise<MediaInfo | null>
  }

  let {
    captureRuntime,
    captureCountdownSeconds = 3,
    captureUploader = doUpload,
    voiceoverRuntime,
    voiceoverCountdownSeconds = 3,
    voiceoverUploader = doUpload,
  }: Props = $props()

  const missingSourceCount = $derived(
    Object.keys(compositionState.document.sources).filter((id) => !compositionState.media[id]).length,
  )
  let captureOpen = $state(false)
  let captureUploading = $state(false)
  let captureError = $state('')
  let voiceoverOpen = $state(false)
  let voiceoverUploading = $state(false)
  let voiceoverMessage = $state('')
  let voiceoverError = $state('')
  let pendingVoiceover = $state.raw<File | null>(null)
  let archivePicker = $state<HTMLInputElement>()
  let archiveBusy = $state(false)
  let archiveMessage = $state('')
  let archiveError = $state('')
  let reviewThreads = $state<ReviewThreadDto[]>([])

  $effect(() => {
    const projectId = compositionState.projectId
    const token = authState.token
    if (!projectId || !token) return
    const timer = window.setInterval(() => void refreshOpenCompositionProject(), 5_000)
    return () => window.clearInterval(timer)
  })

  $effect(() => {
    const token = authState.token
    if (!token) return
    const timer = window.setInterval(() => void refreshCompositionProjects(), 10_000)
    return () => window.clearInterval(timer)
  })

  $effect(() => {
    const token = authState.token
    if (!token || typeof EventSource === 'undefined') return
    return subscribeCompositionProjectChanges(() => {
      void refreshCompositionProjects()
      void refreshOpenCompositionProject()
    })
  })
  let loadedProjectsForToken = $state<string | null>(null)

  $effect(() => syncCompositionLibrary(legacyState.library, legacyState.librarySnapshotReady))
  $effect(() => {
    const token = authState.token
    if (token && loadedProjectsForToken !== token) {
      loadedProjectsForToken = token
      void loadCompositionProjects()
    } else if (!token) {
      loadedProjectsForToken = null
      compositionState.projects = []
    }
  })

  function openSelected(event: Event): void {
    const id = (event.currentTarget as HTMLSelectElement).value
    if (id) void openCompositionProject(id)
  }

  async function addCapture(file: File): Promise<void> {
    if (captureUploading) return
    captureUploading = true
    captureError = ''
    try {
      const media = await captureUploader(file, false)
      if (!media) throw new Error(legacyState.importError || 'Не удалось загрузить запись')
      addMediaInfoToComposition(media)
      captureOpen = false
    } catch (error) {
      captureError = error instanceof Error ? error.message : String(error)
    } finally {
      captureUploading = false
    }
  }

  async function addVoiceover(file: File): Promise<void> {
    if (voiceoverUploading) return
    if (pendingVoiceover && pendingVoiceover !== file) {
      voiceoverError = 'Сначала сохраните предыдущую голосовую запись'
      return
    }
    pendingVoiceover = file
    await uploadPendingVoiceover()
  }

  async function uploadPendingVoiceover(): Promise<void> {
    const file = pendingVoiceover
    if (!file || voiceoverUploading) return
    voiceoverError = ''
    voiceoverMessage = ''
    const validationError = voiceoverFileValidationError(file)
    if (validationError) {
      voiceoverError = validationError
      return
    }
    voiceoverUploading = true
    try {
      const media = await voiceoverUploader(file, false)
      if (!media) throw new Error(legacyState.importError || 'Не удалось загрузить голосовую запись')
      addVoiceoverMediaInfoToComposition(media)
      if (pendingVoiceover === file) pendingVoiceover = null
      voiceoverMessage = 'Голосовая запись добавлена на аудиодорожку'
      voiceoverOpen = false
    } catch (error) {
      voiceoverError = error instanceof Error ? error.message : String(error)
    } finally {
      voiceoverUploading = false
    }
  }

  function downloadPendingVoiceover(): void {
    const file = pendingVoiceover
    if (!file) return
    let url = ''
    try {
      url = URL.createObjectURL(file)
      downloadUrl(url, file.name)
    } catch (error) {
      voiceoverError = error instanceof Error ? error.message : String(error)
    } finally {
      if (url) window.setTimeout(() => URL.revokeObjectURL(url), 0)
    }
  }

  let activeSavedProjectId=$derived(nestedComposition.document?nestedComposition.projectId:compositionState.projectId)
  function downloadProjectArchive(): void {
    const projectId = activeSavedProjectId
    if (!projectId || !authState.token || archiveBusy) return
    archiveError = ''
    archiveMessage = ''
    try {
      downloadUrl(compositionProjectArchiveUrl(projectId))
      archiveMessage = 'Загрузка переносимого архива началась'
    } catch (error) {
      archiveError = error instanceof Error ? error.message : String(error)
    }
  }

  async function importProjectArchive(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file || !authState.token || archiveBusy) return
    archiveBusy = true
    archiveError = ''
    archiveMessage = ''
    try {
      if (file.size > MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES) {
        throw new Error('Архив проекта превышает лимит 2 ГиБ')
      }
      const imported = await importCompositionProjectArchive(file, authState.token, spacesState.selectedId || undefined)
      const [libraryLoaded] = await Promise.all([loadLibrary(), loadCompositionProjects()])
      await openCompositionProject(imported.project.id)
      syncCompositionLibrary(legacyState.library, libraryLoaded)
      archiveMessage = `Импортирован проект «${imported.project.name}» · ${Object.keys(imported.sourceMapping).length} медиа`
    } catch (error) {
      archiveError = error instanceof Error ? error.message : String(error)
    } finally {
      archiveBusy = false
    }
  }

  async function templateCreated(result:import('$lib/api').NativeTemplateImportResponse,guard:()=>void):Promise<void>{
    guard()
    const [libraryLoaded]=await Promise.all([loadLibrary(),loadCompositionProjects()])
    guard()
    await openCompositionProject(result.project.id,guard)
    syncCompositionLibrary(legacyState.library,libraryLoaded)
    archiveMessage=`Создан проект «${result.project.name}» из шаблона`
  }

  function downloadUrl(url: string, filename = ''): void {
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = filename
    anchor.hidden = true
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
  }
</script>

<main class="composition-workspace" class:agent-view={agentSession.view==='agent'}>
  {#if !authState.token}
    <AuthPanel />
  {:else}
    <SpacePanel onselectionchange={() => void loadLibrary()} />
  {/if}
  <section class="composition-projectbar card" aria-label="Проект композиции">
    <button class="btn ghost sm" type="button" onclick={()=>{agentSession.view=agentSession.view==='editor'?'agent':'editor'}}>{agentSession.view==='editor'?'Вид агента':'Полный редактор'}</button>
    {#if agentSession.work}<span role="status">Агент: {agentSession.work.reason}</span>{/if}
    <input
      class="composition-project-name"
      aria-label="Название композиции"
      value={nestedComposition.document ? nestedComposition.name : compositionState.projectName}
      oninput={(event) => { if (nestedComposition.document) nestedComposition.name = event.currentTarget.value; else setCompositionProjectName(event.currentTarget.value) }}
    />
    <button class="btn ghost sm" disabled={compositionState.save.busy} onclick={() => { closeNestedComposition(); newComposition() }}>Новый</button>
    <button class="btn primary sm" disabled={nestedHistory.busy || compositionState.save.busy || !authState.token} onclick={() => void saveCompositionProject()}>
      {compositionState.save.busy ? 'Сохраняю…' : activeSavedProjectId ? 'Сохранить' : 'Создать проект'}
    </button>
    <select aria-label="Сохранённые композиции" disabled={compositionState.save.busy || !authState.token} value={activeSavedProjectId ?? ''} onchange={openSelected}>
      <option value="">Открыть проект…</option>
      {#each compositionState.projects as project (project.id)}
        <option value={project.id}>{project.name}</option>
      {/each}
    </select>
    {#if activeSavedProjectId}
      <button class="btn ghost sm" disabled={archiveBusy || !authState.token} onclick={downloadProjectArchive}>Экспорт .veproj</button>
      {#if nestedComposition.document}<NativeTemplateDownload projectId={activeSavedProjectId} disabled={archiveBusy||!authState.token}/>{/if}
      <button class="btn ghost sm danger" disabled={Boolean(nestedComposition.document) || compositionState.save.busy || !authState.token} onclick={() => void deleteCompositionProject(compositionState.projectId!)}>Удалить проект</button>
    {/if}
    <button class="btn ghost sm" disabled={archiveBusy || !authState.token} onclick={() => archivePicker?.click()}>Импорт .veproj</button>
    {#if authState.user}
      <span class="composition-project-user">{authState.user.username}</span>
      <button class="btn ghost sm" onclick={() => void logout()}>Выйти</button>
    {/if}
    <input
      bind:this={archivePicker}
      class="composition-archive-picker"
      type="file"
      accept=".veproj,application/vnd.video-editor.project"
      onchange={(event) => void importProjectArchive(event)}
    />
  </section>

  <label class="composition-import-preference">
    <input type="checkbox" checked={importPreferences.autoPairAudio} onchange={(event) => setAutoPairAudioOnImport(event.currentTarget.checked)} />
    Отделять и связывать аудио при импорте видео
  </label>

  {#if compositionState.save.error}<p class="composition-workspace-error error" role="alert">{compositionState.save.error}</p>{/if}
  {#if archiveMessage}<p class="composition-archive-message" role="status">{archiveMessage}</p>{/if}
  {#if archiveError}<p class="composition-workspace-error error" role="alert">{archiveError}</p>{/if}
  {#if missingSourceCount}
    <p class="composition-source-warning" role="status">
      Не найдено медиафайлов: {missingSourceCount}. Дорожки и клипы сохранены; восстановите файлы в медиатеке.
    </p>
  {/if}

  <details class="composition-capture card" bind:open={captureOpen}>
    <summary>Записать экран, окно или вкладку</summary>
    {#if captureOpen}
      <CapturePanel
        title="Запись для композиции"
        runtime={captureRuntime}
        countdownSeconds={captureCountdownSeconds}
        oncapture={addCapture}
        oncaptureerror={(message) => { captureError = message }}
      />
      {#if captureUploading}<p class="composition-capture-status" role="status">Загружаю запись в медиатеку и добавляю на timeline…</p>{/if}
      {#if captureError}<p class="error" role="alert">{captureError}</p>{/if}
    {/if}
  </details>

  <details class="composition-voiceover card" bind:open={voiceoverOpen}>
    <summary>Записать голос на аудиодорожку</summary>
    {#if voiceoverOpen}
      <VoiceoverPanel
        title="Voiceover для композиции"
        runtime={voiceoverRuntime}
        countdownSeconds={voiceoverCountdownSeconds}
        busy={voiceoverUploading}
        blocked={Boolean(pendingVoiceover)}
        oncapture={addVoiceover}
        onvoiceovererror={(message) => { voiceoverError = message }}
      />
      {#if voiceoverUploading}<p class="composition-voiceover-status" role="status">Загружаю голосовую запись…</p>{/if}
      {#if voiceoverError}<p class="error" role="alert">{voiceoverError}</p>{/if}
    {/if}
  </details>
  {#if pendingVoiceover}
    <section class="composition-voiceover-pending card" aria-label="Несохранённая голосовая запись">
      <p role="status">
        <strong>{pendingVoiceover.name}</strong> хранится локально до успешной загрузки.
      </p>
      <button class="btn primary sm" type="button" disabled={voiceoverUploading || Boolean(voiceoverFileValidationError(pendingVoiceover))} onclick={() => void uploadPendingVoiceover()}>
        Повторить загрузку
      </button>
      <button class="btn ghost sm" type="button" onclick={downloadPendingVoiceover}>Скачать запись</button>
    </section>
  {/if}
  {#if voiceoverMessage}<p class="composition-voiceover-message" role="status">{voiceoverMessage}</p>{/if}
  {#if voiceoverError && !voiceoverOpen}<p class="composition-workspace-error error" role="alert">{voiceoverError}</p>{/if}

  <NativeTemplateLibrary token={authState.token} spaceId={spacesState.selectedId||undefined} context={nestedComposition.document??compositionState.document} disabled={compositionState.save.busy||nestedHistory.busy||archiveBusy} oncreated={templateCreated} sources={legacyState.library.flatMap(entry=>{try{return[{label:entry.filename,source:compositionSourceFromLibraryEntry(entry)}]}catch{return[]}})}/>
  <NativeTemplateImport token={authState.token} spaceId={spacesState.selectedId||undefined} context={nestedComposition.document??compositionState.document} disabled={compositionState.save.busy||nestedHistory.busy||archiveBusy} oncreated={templateCreated} sources={legacyState.library.flatMap(entry=>{try{return[{label:entry.filename,source:compositionSourceFromLibraryEntry(entry)}]}catch{return[]}})}/>
  <ConcatProjectImportPanel token={authState.token} context={compositionState.document} disabled={compositionState.save.busy} sources={legacyState.library.flatMap(entry=>{try{return[{label:entry.filename,source:compositionSourceFromLibraryEntry(entry)}]}catch{return[]}})} />
  <ConcatProjectExportPanel disabled={compositionState.save.busy} />
  <FilmcraftProjectPanel token={authState.token} library={legacyState.library} />

  {#if authState.token}<NestedHistoryPanel disabled={compositionState.save.busy} />{/if}
  {#if !nestedComposition.document&&!nestedHistory.future.length}<OrdinaryHistoryPanel disabled={compositionState.save.busy} />{/if}
  {#if nestedComposition.document && authState.token}
    <CaptionStylePanel />
    <NestedCaptionExport />
    <section class="native-agent-preview" aria-label="Вложенная композиция">
      <button class="btn ghost sm" onclick={closeNestedComposition}>Вернуться к локальному монтажу</button>
      <button class="btn ghost sm" disabled={nestedHistory.busy || compositionState.save.busy} onclick={() => void groupNestedDocument(nestedComposition.activeCompositionId ?? nestedComposition.document!.root, `group-${crypto.randomUUID()}`, `instance-${crypto.randomUUID()}`)}>Сгруппировать композицию</button>
      <button class="btn ghost sm" disabled={nestedHistory.busy || compositionState.save.busy || !nestedHistory.past.length} onclick={undoNestedComposition}>Отменить</button>
      <button class="btn ghost sm" disabled={nestedHistory.busy || compositionState.save.busy || !nestedHistory.future.length} onclick={redoNestedComposition}>Повторить</button>
      {#if nestedHistory.error}<p role="alert">{nestedHistory.error}</p>{/if}
      <NestedTimelineControls disabled={compositionState.save.busy} />
      <NestedMarkers disabled={compositionState.save.busy} />
      <NestedTrackControls disabled={compositionState.save.busy} />
      <NestedMediaInsert disabled={compositionState.save.busy} />
      <NestedProjectMedia disabled={compositionState.save.busy} token={authState.token} sources={legacyState.library.flatMap(entry=>{try{return[{label:entry.filename,source:compositionSourceFromLibraryEntry(entry)}]}catch{return[]}})} />
      <NestedClipTransfer disabled={compositionState.save.busy} />
      <NestedConcatTitleInsert disabled={compositionState.save.busy} />
      <NestedConcatTitleInspector disabled={compositionState.save.busy} />
      <NestedConcatShapeInsert disabled={compositionState.save.busy} />
      <NestedConcatShapeInspector disabled={compositionState.save.busy} />
      <NestedConcatTreatmentInsert disabled={compositionState.save.busy} />
      <NestedConcatTreatmentInspector disabled={compositionState.save.busy} />
      <NestedMotionPathInspector disabled={compositionState.save.busy} />
      <NestedScalarTimeline disabled={compositionState.save.busy} />
      <NestedScalarKeyInspector disabled={compositionState.save.busy} />
      <NestedEmbeddedAudioInspector disabled={compositionState.save.busy} />
      <NestedAudioStreamInspector disabled={compositionState.save.busy} />
      <NestedConcatGainInspector disabled={compositionState.save.busy} />
      <NestedDetachedAudioInspector disabled={compositionState.save.busy} />
      <NativeSpeechPanel token={authState.token} disabled={compositionState.save.busy} />
      <NestedPlaybackControls token={authState.token} disabled={compositionState.save.busy || nestedHistory.busy} />
      <label>Время, мкс <input type="number" min="0" step="1" bind:value={nestedComposition.timeUs} /></label>
      {#each nestedComposition.document.compositions[nestedComposition.activeCompositionId ?? nestedComposition.document.root]?.instances ?? [] as instance (instance.id)}
        <button class="btn ghost sm" disabled={nestedHistory.busy || compositionState.save.busy} onclick={() => void splitNestedInstance(nestedComposition.activeCompositionId ?? nestedComposition.document!.root, instance.id, `instance-${crypto.randomUUID()}`, nestedComposition.timeUs)}>Разрезать {instance.id} по курсору</button>
      {/each}
      {#each Object.keys(nestedComposition.document.compositions) as id (id)}
        <label>{id}
          <select aria-label={`Видео композиции ${id}`} disabled={nestedHistory.busy || compositionState.save.busy} value={nestedComposition.renderedOutputs[id] ?? ''} onchange={(event) => { const outputs = { ...nestedComposition.renderedOutputs }; if (event.currentTarget.value) outputs[id] = event.currentTarget.value; else delete outputs[id]; nestedComposition.renderedOutputs = outputs }}>
            <option value="">Автоматическое превью</option>
            {#each legacyState.library.filter((entry) => entry.kind === 'output') as entry (entry.id)}
              <option value={entry.id}>{entry.title}</option>
            {/each}
          </select>
        </label>
      {/each}
      <NestedExportPanel token={authState.token} disabled={compositionState.save.busy} />
      <ProjectFonts disabled={compositionState.save.busy} />
      <NestedEffectStack disabled={compositionState.save.busy} />
      <NestedLutInspector disabled={compositionState.save.busy} />
      <NestedCreativeGradeInspector disabled={compositionState.save.busy} />
      <NestedToneCurvesInspector disabled={compositionState.save.busy} />
      <NestedProcAmpInspector disabled={compositionState.save.busy} /><NestedDirectionalBlurInspector disabled={compositionState.save.busy} /><NestedDirectionalBlurLegacyInspector disabled={compositionState.save.busy} /><NestedNoiseInspector disabled={compositionState.save.busy} /><NestedNoiseLegacyInspector disabled={compositionState.save.busy} /><NestedStrobeInspector disabled={compositionState.save.busy} /><NestedReplicateInspector disabled={compositionState.save.busy} /><NestedBrushStrokesInspector disabled={compositionState.save.busy} /><NestedRoughenEdgesInspector disabled={compositionState.save.busy} /><NestedLongShadowInspector disabled={compositionState.save.busy} /><NestedAlphaGlowInspector disabled={compositionState.save.busy} /><NestedMoveInspector disabled={compositionState.save.busy} /><NestedWiggleInspector disabled={compositionState.save.busy} /><NestedCameraShakeInspector disabled={compositionState.save.busy} /><NestedSpacerInspector disabled={compositionState.save.busy} /><NestedCompoundBlurInspector disabled={compositionState.save.busy} /><NestedTrackMatteInspector disabled={compositionState.save.busy} /><NestedGraphicTextInspector disabled={compositionState.save.busy} />
<NestedGraphicShapeInspector disabled={compositionState.save.busy} /><NestedBokehBlurInspector disabled={compositionState.save.busy} /><NestedUnsharpMaskInspector disabled={compositionState.save.busy} /><NestedCropInspector disabled={compositionState.save.busy} /><NestedTransformEffectInspector disabled={compositionState.save.busy} /><NestedStandaloneVignetteInspector disabled={compositionState.save.busy} /><NestedLogoCutoutInspector disabled={compositionState.save.busy} /><NestedLightingEffectsInspector disabled={compositionState.save.busy} /><NestedAscCdlInspector disabled={compositionState.save.busy} /><NestedBrightnessContrastInspector disabled={compositionState.save.busy} /><NestedFocusBlurInspector disabled={compositionState.save.busy} /><NestedTurbulentDisplaceInspector disabled={compositionState.save.busy} /><NestedChannelBlurInspector disabled={compositionState.save.busy} /><NestedReduceInterlaceFlickerInspector disabled={compositionState.save.busy} /><NestedStandaloneSharpenInspector disabled={compositionState.save.busy} /><NestedEdgeFeatherInspector disabled={compositionState.save.busy} /><NestedAutoAlignInspector disabled={compositionState.save.busy} /><NestedCloneInspector disabled={compositionState.save.busy} /><NestedRoundedCropInspector disabled={compositionState.save.busy} /><NestedSpinInspector disabled={compositionState.save.busy} /><NestedGrowInspector disabled={compositionState.save.busy} /><NestedShrinkInspector disabled={compositionState.save.busy} /><NestedRotate3DInspector disabled={compositionState.save.busy} /><NestedCornerPinInspector disabled={compositionState.save.busy} /><NestedOffsetInspector disabled={compositionState.save.busy} /><NestedVolumetricRaysInspector disabled={compositionState.save.busy} /><NestedVrChromaticInspector disabled={compositionState.save.busy} /><NestedVrPlaneToSphereInspector disabled={compositionState.save.busy} /><NestedLightningInspector disabled={compositionState.save.busy} /><NestedWriteOnInspector disabled={compositionState.save.busy} /><NestedTimecodeInspector disabled={compositionState.save.busy} /><NestedClipNameInspector disabled={compositionState.save.busy} /><NestedSimpleTextInspector disabled={compositionState.save.busy} /><NestedMetadataBurninInspector disabled={compositionState.save.busy} /><NestedVrGlowInspector disabled={compositionState.save.busy} /><NestedVrFractalNoiseInspector disabled={compositionState.save.busy} /><NestedVrDigitalGlitchInspector disabled={compositionState.save.busy} /><NestedVrColorGradientsInspector disabled={compositionState.save.busy} /><NestedVrDenoiseInspector disabled={compositionState.save.busy} /><NestedVrBlurInspector disabled={compositionState.save.busy} /><NestedVrSharpenInspector disabled={compositionState.save.busy} /><NestedSphericalVideoInspector disabled={compositionState.save.busy} media={legacyState.library} token={authState.token} /><NestedVrProjectionInspector disabled={compositionState.save.busy} /><NestedVrRotateSphereInspector disabled={compositionState.save.busy} /><NestedLightLeaksInspector disabled={compositionState.save.busy} /><NestedWarpStabilizerInspector disabled={compositionState.save.busy} /><NestedAutoReframeInspector disabled={compositionState.save.busy} /><NestedEchoInspector disabled={compositionState.save.busy} /><NestedPosterizeTimeInspector disabled={compositionState.save.busy} /><NestedGlintInspector disabled={compositionState.save.busy} /><NestedWonderGlowInspector disabled={compositionState.save.busy} /><NestedEchoGlowInspector disabled={compositionState.save.busy} /><NestedEdgeGlowInspector disabled={compositionState.save.busy} /><NestedRgbSplitInspector disabled={compositionState.save.busy} /><NestedMirrorInspector disabled={compositionState.save.busy} /><NestedSpherizeInspector disabled={compositionState.save.busy} /><NestedMagnifyInspector disabled={compositionState.save.busy} /><NestedLinearWipeInspector disabled={compositionState.save.busy} /><NestedGradientWipeInspector disabled={compositionState.save.busy} /><NestedBlockDissolveInspector disabled={compositionState.save.busy} /><NestedStrokeInspector disabled={compositionState.save.busy} /><NestedColorEmbossInspector disabled={compositionState.save.busy} /><NestedGradientInspector disabled={compositionState.save.busy} /><NestedPaintBucketInspector disabled={compositionState.save.busy} /><NestedEllipseInspector disabled={compositionState.save.busy} /><NestedCellPatternInspector disabled={compositionState.save.busy} /><NestedCheckerboardInspector disabled={compositionState.save.busy} /><NestedLensFlareInspector disabled={compositionState.save.busy} /><NestedGridInspector disabled={compositionState.save.busy} /><NestedCircleInspector disabled={compositionState.save.busy} /><NestedRampInspector disabled={compositionState.save.busy} /><NestedFourColorGradientInspector disabled={compositionState.save.busy} /><NestedBevelAlphaInspector disabled={compositionState.save.busy} /><NestedDropShadowInspector disabled={compositionState.save.busy} /><NestedBasic3DInspector disabled={compositionState.save.busy} /><NestedLensDistortionInspector disabled={compositionState.save.busy} /><NestedWaveWarpInspector disabled={compositionState.save.busy} /><NestedTwirlInspector disabled={compositionState.save.busy} /><NestedTwirlLegacyInspector disabled={compositionState.save.busy} /><NestedEmbossInspector disabled={compositionState.save.busy} /><NestedFindEdgesInspector disabled={compositionState.save.busy} /><NestedMosaicInspector disabled={compositionState.save.busy} /><NestedMosaicLegacyInspector disabled={compositionState.save.busy} /><NestedMedianInspector disabled={compositionState.save.busy} /><NestedCameraBlurInspector disabled={compositionState.save.busy} /><NestedGaussianBlurInspector disabled={compositionState.save.busy} /><NestedGaussianBlurLegacyInspector disabled={compositionState.save.busy} /><NestedGammaInspector disabled={compositionState.save.busy} /><NestedPosterizeInspector disabled={compositionState.save.busy} /><NestedInvertInspector disabled={compositionState.save.busy} /><NestedLevelsInspector disabled={compositionState.save.busy} /><NestedExtractInspector disabled={compositionState.save.busy} /><NestedBlackWhiteInspector disabled={compositionState.save.busy} /><NestedHorizontalFlipInspector disabled={compositionState.save.busy} /><NestedVerticalFlipInspector disabled={compositionState.save.busy} /><NestedTintInspector disabled={compositionState.save.busy} /><NestedColorBalanceInspector disabled={compositionState.save.busy} /><NestedLeaveColorInspector disabled={compositionState.save.busy} /><NestedChangeToColorInspector disabled={compositionState.save.busy} /><NestedUltraKeyInspector disabled={compositionState.save.busy} /><NestedColorKeyInspector disabled={compositionState.save.busy} /><NestedLumaKeyInspector disabled={compositionState.save.busy} /><NestedAlphaAdjustInspector disabled={compositionState.save.busy} /><NestedChannelMixInspector disabled={compositionState.save.busy} /><NestedColorReplaceInspector disabled={compositionState.save.busy} /><NestedColorPassInspector disabled={compositionState.save.busy} /><NestedCineonInspector disabled={compositionState.save.busy} /><NestedVideoLimiterInspector disabled={compositionState.save.busy} /><NestedHslInspector disabled={compositionState.save.busy} /><NestedVignetteInspector disabled={compositionState.save.busy} /><NestedLumetriInspector token={authState.token} disabled={compositionState.save.busy} /><NestedBasicGradeInspector disabled={compositionState.save.busy} /><NestedCreativeAdjustInspector disabled={compositionState.save.busy} /><NestedSharpenInspector disabled={compositionState.save.busy} /><NestedConcatContrastInspector disabled={compositionState.save.busy} /><NestedConcatExposureInspector disabled={compositionState.save.busy} /><NestedConcatMotionBlurInspector disabled={compositionState.save.busy} /><NestedConcatBloomPulseInspector disabled={compositionState.save.busy} /><NestedConcatZoomBlurInspector disabled={compositionState.save.busy} /><NestedConcatFilmGrainInspector disabled={compositionState.save.busy} /><NestedConcatGlowInspector disabled={compositionState.save.busy} /><NestedConcatHalftoneInspector disabled={compositionState.save.busy} /><NestedConcatFisheyeInspector disabled={compositionState.save.busy} /><NestedConcatChromaticAberrationInspector disabled={compositionState.save.busy} /><NestedConcatPosterizeInspector disabled={compositionState.save.busy} /><NestedConcatGaussianBlurInspector disabled={compositionState.save.busy} /><NestedConcatPixelateInspector disabled={compositionState.save.busy} /><NestedConcatLumaKeyInspector disabled={compositionState.save.busy} /><NestedConcatChromaKeyInspector disabled={compositionState.save.busy} /><NestedConcatSaturationInspector disabled={compositionState.save.busy} /><NestedConcatVibranceInspector disabled={compositionState.save.busy} /><NestedConcatWhiteBalanceInspector disabled={compositionState.save.busy} /><NestedConcatSpatialInspector token={authState.token} disabled={compositionState.save.busy} /><NestedConcatAdjustInspector token={authState.token} disabled={compositionState.save.busy} /><NestedConcatCurvesInspector token={authState.token} disabled={compositionState.save.busy} /><NestedConcatColorWheelsInspector token={authState.token} disabled={compositionState.save.busy} />
      <NestedChromaInspector disabled={compositionState.save.busy} /><NestedMaskInspector disabled={compositionState.save.busy} />
      <NestedScopes project={nestedComposition.document} timeUs={nestedComposition.timeUs} renderedOutputs={nestedComposition.renderedOutputs} token={authState.token} compositionId={nestedComposition.activeCompositionId ?? nestedComposition.document.root} />
      <div class="native-preview-stage" class:comparison={nestedComparison.enabled}>
      {#if nestedComparison.enabled}<div><span>Эталон</span>
      <NestedCompositionPreview savedProject={nestedComposition.projectId&&nestedComposition.revision!==null?{projectId:nestedComposition.projectId,expectedRevision:nestedComposition.revision}:undefined} compositionId={nestedComposition.activeCompositionId ?? nestedComposition.document.root} project={nestedComposition.document} label="Эталон Color Match" timeUs={nestedComparison.referenceUs} renderedOutputs={nestedComposition.renderedOutputs} token={authState.token} /></div>{/if}
      <div>{#if nestedComparison.enabled}<span>Текущий кадр</span>{/if}
      <NestedFrameExport token={authState.token} />
      <NestedCompositionPreview savedProject={nestedComposition.projectId&&nestedComposition.revision!==null?{projectId:nestedComposition.projectId,expectedRevision:nestedComposition.revision}:undefined} compositionId={nestedComposition.activeCompositionId ?? nestedComposition.document.root} project={nestedComposition.document} effectDraft playing={nestedPlayback.playing} playbackEpoch={nestedPlayback.epoch} timeUs={nestedComposition.timeUs} renderedOutputs={nestedComposition.renderedOutputs} token={authState.token}>
        {#snippet overlay()}<NestedMotionPathOverlay disabled={compositionState.save.busy} />{/snippet}
      </NestedCompositionPreview></div>
      </div>
    </section>
  {:else}
  <div class="composition-workspace-grid">
    <CompositionPreview />
    <CompositionInspector />
  </div>
  <LazyShotReview />
  <CompositionLinks />
  <CompositionSearch />
  <MultiTrackTimeline {reviewThreads} />
  <CompositionReviewPanel onthreadschange={(threads) => { reviewThreads = threads }} />
  <CompositionLocalTools />
  <YouTubePublishPanel />
  {/if}
  {#if !nestedComposition.document || authState.token}<AgentPanel />{/if}
</main>

<style>
  .agent-view > :global(:not(.composition-projectbar):not(.composition-workspace-grid):not(.native-agent-preview):not(.agent-panel):not([role="alert"]):not([role="status"])) { display:none; }
  .agent-view > .composition-workspace-grid { grid-template-columns:1fr; }
  .agent-view > .composition-workspace-grid > :global(.composition-inspector) { display:none; }
  .agent-view > .native-agent-preview > :global(:not(.native-preview-stage):not(.composition-transport):not([role="alert"]):not([role="status"])) { display:none; }

  .composition-archive-picker { display: none; }
 .comparison {display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:0.5rem;}
</style>
