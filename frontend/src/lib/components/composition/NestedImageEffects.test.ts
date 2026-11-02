import SphericalVideo from './NestedSphericalVideoInspector.svelte'
import GraphicText from './NestedGraphicTextInspector.svelte'
import GraphicShape from './NestedGraphicShapeInspector.svelte'
import TrackMatte from './NestedTrackMatteInspector.svelte'
import Lightning from './NestedLightningInspector.svelte'
import WriteOn from './NestedWriteOnInspector.svelte'
import Timecode from './NestedTimecodeInspector.svelte'
import ClipName from './NestedClipNameInspector.svelte'
// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { editGraphicTextRuns, evaluateNestedFrame, listBuiltinLuts, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Basic from './NestedBasicGradeInspector.svelte'
import Sharpen from './NestedSharpenInspector.svelte'
import Chroma from './NestedChromaInspector.svelte'
import Lumetri from './NestedLumetriInspector.svelte'
import Vignette from './NestedVignetteInspector.svelte'
import Hsl from './NestedHslInspector.svelte'
import Creative from './NestedCreativeGradeInspector.svelte'
import Adjust from './NestedCreativeAdjustInspector.svelte'
import Curves from './NestedToneCurvesInspector.svelte'
import Masks from './NestedMaskInspector.svelte'
import Stack from './NestedEffectStack.svelte'
import Extract from './NestedExtractInspector.svelte'
import BlackWhite from './NestedBlackWhiteInspector.svelte'
import HorizontalFlip from './NestedHorizontalFlipInspector.svelte'
import VerticalFlip from './NestedVerticalFlipInspector.svelte'
import Tint from './NestedTintInspector.svelte'
import Balance from './NestedColorBalanceInspector.svelte'
import LeaveColor from './NestedLeaveColorInspector.svelte'
import ChangeToColor from './NestedChangeToColorInspector.svelte'
import UltraKey from './NestedUltraKeyInspector.svelte'
import ColorKey from './NestedColorKeyInspector.svelte'
import DirectionalBlur from './NestedDirectionalBlurInspector.svelte'
import DirectionalBlurLegacy from './NestedDirectionalBlurLegacyInspector.svelte'
import Noise from './NestedNoiseInspector.svelte'
import NoiseLegacy from './NestedNoiseLegacyInspector.svelte'
import Strobe from './NestedStrobeInspector.svelte'
import Replicate from './NestedReplicateInspector.svelte'
import BrushStrokes from './NestedBrushStrokesInspector.svelte'
import RoughenEdges from './NestedRoughenEdgesInspector.svelte'
import LongShadow from './NestedLongShadowInspector.svelte'
import AlphaGlow from './NestedAlphaGlowInspector.svelte'
import BlockDissolve from './NestedBlockDissolveInspector.svelte'
import GradientWipe from './NestedGradientWipeInspector.svelte'
import LinearWipe from './NestedLinearWipeInspector.svelte'
import Magnify from './NestedMagnifyInspector.svelte'
import Offset from './NestedOffsetInspector.svelte'
import RgbSplit from './NestedRgbSplitInspector.svelte'
import Mirror from './NestedMirrorInspector.svelte'
import Spherize from './NestedSpherizeInspector.svelte'
import CornerPin from './NestedCornerPinInspector.svelte'
import Rotate3D from './NestedRotate3DInspector.svelte'
import Grow from './NestedGrowInspector.svelte'
import Spin from './NestedSpinInspector.svelte'
import Move from './NestedMoveInspector.svelte'
import Wiggle from './NestedWiggleInspector.svelte'
import CameraShake from './NestedCameraShakeInspector.svelte'
import Spacer from './NestedSpacerInspector.svelte'
import CompoundBlur from './NestedCompoundBlurInspector.svelte'
import BokehBlur from './NestedBokehBlurInspector.svelte'
import UnsharpMask from './NestedUnsharpMaskInspector.svelte'
import Crop from './NestedCropInspector.svelte'
import TransformEffect from './NestedTransformEffectInspector.svelte'
import StandaloneVignette from './NestedStandaloneVignetteInspector.svelte'
import EchoGlow from './NestedEchoGlowInspector.svelte'
import VolumetricRays from './NestedVolumetricRaysInspector.svelte'
import VrChromatic from './NestedVrChromaticInspector.svelte'
import VrPlaneToSphere from './NestedVrPlaneToSphereInspector.svelte'
import SimpleText from './NestedSimpleTextInspector.svelte'
import MetadataBurnin from './NestedMetadataBurninInspector.svelte'
import VrGlow from './NestedVrGlowInspector.svelte'
import VrFractalNoise from './NestedVrFractalNoiseInspector.svelte'
import VrDigitalGlitch from './NestedVrDigitalGlitchInspector.svelte'
import VrColorGradients from './NestedVrColorGradientsInspector.svelte'
import VrDenoise from './NestedVrDenoiseInspector.svelte'
import VrBlur from './NestedVrBlurInspector.svelte'
import VrSharpen from './NestedVrSharpenInspector.svelte'
import VrProjection from './NestedVrProjectionInspector.svelte'
import VrRotateSphere from './NestedVrRotateSphereInspector.svelte'
import LightLeaks from './NestedLightLeaksInspector.svelte'
import WarpStabilizer from './NestedWarpStabilizerInspector.svelte'
import AutoReframe from './NestedAutoReframeInspector.svelte'
import Echo from './NestedEchoInspector.svelte'
import PosterizeTime from './NestedPosterizeTimeInspector.svelte'
import Glint from './NestedGlintInspector.svelte'
import WonderGlow from './NestedWonderGlowInspector.svelte'
import EdgeGlow from './NestedEdgeGlowInspector.svelte'
import LogoCutout from './NestedLogoCutoutInspector.svelte'
import LightingEffects from './NestedLightingEffectsInspector.svelte'
import AscCdl from './NestedAscCdlInspector.svelte'
import BrightnessContrast from './NestedBrightnessContrastInspector.svelte'
import FocusBlur from './NestedFocusBlurInspector.svelte'
import TurbulentDisplace from './NestedTurbulentDisplaceInspector.svelte'
import ChannelBlur from './NestedChannelBlurInspector.svelte'
import ReduceInterlaceFlicker from './NestedReduceInterlaceFlickerInspector.svelte'
import StandaloneSharpen from './NestedStandaloneSharpenInspector.svelte'
import EdgeFeather from './NestedEdgeFeatherInspector.svelte'
import AutoAlign from './NestedAutoAlignInspector.svelte'
import Clone from './NestedCloneInspector.svelte'
import RoundedCrop from './NestedRoundedCropInspector.svelte'
import Shrink from './NestedShrinkInspector.svelte'
import Stroke from './NestedStrokeInspector.svelte'
import ColorEmboss from './NestedColorEmbossInspector.svelte'
import Gradient from './NestedGradientInspector.svelte'
import PaintBucket from './NestedPaintBucketInspector.svelte'
import Ellipse from './NestedEllipseInspector.svelte'
import CellPattern from './NestedCellPatternInspector.svelte'
import Checkerboard from './NestedCheckerboardInspector.svelte'
import LensFlare from './NestedLensFlareInspector.svelte'
import Grid from './NestedGridInspector.svelte'
import Circle from './NestedCircleInspector.svelte'
import Ramp from './NestedRampInspector.svelte'
import FourColorGradient from './NestedFourColorGradientInspector.svelte'
import BevelAlpha from './NestedBevelAlphaInspector.svelte'
import DropShadow from './NestedDropShadowInspector.svelte'
import Basic3D from './NestedBasic3DInspector.svelte'
import LensDistortion from './NestedLensDistortionInspector.svelte'
import WaveWarp from './NestedWaveWarpInspector.svelte'
import Twirl from './NestedTwirlInspector.svelte'
import TwirlLegacy from './NestedTwirlLegacyInspector.svelte'
import Emboss from './NestedEmbossInspector.svelte'
import FindEdges from './NestedFindEdgesInspector.svelte'
import Mosaic from './NestedMosaicInspector.svelte'
import MosaicLegacy from './NestedMosaicLegacyInspector.svelte'
import Median from './NestedMedianInspector.svelte'
import CameraBlur from './NestedCameraBlurInspector.svelte'
import GaussianBlur from './NestedGaussianBlurInspector.svelte'
import GaussianBlurLegacy from './NestedGaussianBlurLegacyInspector.svelte'
import LumaKey from './NestedLumaKeyInspector.svelte'
import AlphaAdjust from './NestedAlphaAdjustInspector.svelte'
import ChannelMix from './NestedChannelMixInspector.svelte'
import ColorReplace from './NestedColorReplaceInspector.svelte'
import ColorPass from './NestedColorPassInspector.svelte'
vi.mock('$lib/api', () => ({ editGraphicTextRuns:vi.fn(), evaluateNestedFrame: vi.fn(), listBuiltinLuts: vi.fn().mockResolvedValue([]), sampleToneCurve: vi.fn().mockResolvedValue(Array.from({length:256},(_,i)=>i/255)) }))
let component: ReturnType<typeof mount> | undefined
beforeEach(() => {
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'image',id:'images',name:'Images',locked:false,clips:[{id:'photo',timelineStartTick:100000}]}]},instances:[]}}} as unknown as NativeNestedProject
})
afterEach(async () => {if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
it.each([
 [SphericalVideo,'Добавить 360° видео','spherical_video'],
 [GraphicText,'Добавить GraphicText','graphic_text'],
 [GraphicShape,'Добавить GraphicShape','graphic_shape'],
 [TrackMatte,'Добавить Track Matte Key','track_matte'],
 [DirectionalBlur,'Добавить Directional Blur','directional_blur'],
 [DirectionalBlurLegacy,'Добавить Directional Blur Legacy','directional_blur_legacy'],
 [Noise,'Добавить Noise','noise'],
 [NoiseLegacy,'Добавить Noise Legacy','noise_legacy'],
 [Strobe,'Добавить Strobe Light','strobe'],
 [Replicate,'Добавить Replicate','replicate'],
 [BrushStrokes,'Добавить BrushStrokes','brush_strokes'],
 [RoughenEdges,'Добавить RoughenEdges','roughen_edges'],
 [LongShadow,'Добавить LongShadow','long_shadow'],
 [AlphaGlow,'Добавить AlphaGlow','alpha_glow'],
 [BlockDissolve,'Добавить BlockDissolve','block_dissolve'],
 [GradientWipe,'Добавить GradientWipe','gradient_wipe'],
 [LinearWipe,'Добавить LinearWipe','linear_wipe'],
 [Magnify,'Добавить Magnify','magnify'],
 [Offset,'Добавить Offset','offset'],
 [RgbSplit,'Добавить RGB Split','rgb_split'],
 [Mirror,'Добавить Mirror','mirror'],
 [Spherize,'Добавить Spherize','spherize'],
 [CornerPin,'Добавить CornerPin','corner_pin'],
 [Rotate3D,'Добавить Rotate3D','rotate_3d'],
 [Grow,'Добавить Grow','grow'],
 [Spin,'Добавить Spin','spin'],
 [Move,'Добавить Move','move'],
 [Wiggle,'Добавить Wiggle','wiggle'],
 [CameraShake,'Добавить Camera Shake','camera_shake'],
 [Spacer,'Добавить Spacer','spacer'],
 [CompoundBlur,'Добавить Compound Blur','compound_blur'],
 [BokehBlur,'Добавить Bokeh Blur','bokeh_blur'],
 [UnsharpMask,'Добавить Unsharp Mask','unsharp_mask'],
 [Crop,'Добавить Crop','crop'],
 [TransformEffect,'Добавить Transform','transform'],
 [StandaloneVignette,'Добавить Vignette','vignette'],
 [EchoGlow,'Добавить Echo Glow','echo_glow'],
 [VolumetricRays,'Добавить Volumetric Rays','volumetric_rays'],
 [VrChromatic,'Добавить VR Chromatic Aberrations','vr_chromatic_aberrations'],
 [VrPlaneToSphere,'Добавить VR Plane to Sphere','vr_plane_to_sphere'],
 [SimpleText,'Добавить Simple Text','simple_text'],
 [MetadataBurnin,'Добавить Metadata Burn-in','metadata_burnin'],
 [VrGlow,'Добавить VR Glow','vr_glow'],
 [VrFractalNoise,'Добавить VR Fractal Noise','vr_fractal_noise'],
 [VrDigitalGlitch,'Добавить VR Digital Glitch','vr_digital_glitch'],
 [VrColorGradients,'Добавить VR Color Gradients','vr_color_gradients'],
 [VrDenoise,'Добавить VR De-Noise','vr_denoise'],
 [VrBlur,'Добавить VR Blur','vr_blur'],
 [VrSharpen,'Добавить VR Sharpen','vr_sharpen'],
 [VrProjection,'Добавить VR Projection','vr_projection'],
 [VrRotateSphere,'Добавить VR Rotate Sphere','vr_rotate_sphere'],
 [LightLeaks,'Добавить Light Leaks','light_leaks'],
 [WarpStabilizer,'Добавить Warp Stabilizer','warp_stabilizer'],
 [AutoReframe,'Добавить Auto Reframe','auto_reframe'],
 [Echo,'Добавить Echo','echo'],
 [PosterizeTime,'Добавить Posterize Time','posterize_time'],
 [Glint,'Добавить Glint','glint'],
 [WonderGlow,'Добавить Wonder Glow','wonder_glow'],
 [EdgeGlow,'Добавить Edge Glow','edge_glow'],
 [LogoCutout,'Добавить Logo Cutout','logo_cutout'],
 [LightingEffects,'Добавить Lighting Effects','lighting_effects'],
 [AscCdl,'Добавить ASC CDL','asc_cdl'],
 [BrightnessContrast,'Добавить Brightness & Contrast','brightness_contrast'],
 [FocusBlur,'Добавить Focus Blur','focus_blur'],
 [TurbulentDisplace,'Добавить Turbulent Displace','turbulent_displace'],
 [ChannelBlur,'Добавить Channel Blur','channel_blur'],
 [ReduceInterlaceFlicker,'Добавить Reduce Interlace Flicker','reduce_interlace_flicker'],
 [StandaloneSharpen,'Добавить Sharpen','sharpen'],
 [EdgeFeather,'Добавить Edge Feather','edge_feather'],
 [AutoAlign,'Добавить Auto Align','auto_align'],
 [Clone,'Добавить Clone','clone'],
 [RoundedCrop,'Добавить Rounded Crop','rounded_crop'],
 [Shrink,'Добавить Shrink','shrink'],
 [Stroke,'Добавить Stroke','stroke'],
 [ColorEmboss,'Добавить ColorEmboss','color_emboss'],
 [Gradient,'Добавить Gradient','gradient'],
 [PaintBucket,'Добавить PaintBucket','paint_bucket'],
 [Ellipse,'Добавить Ellipse','ellipse'],
 [CellPattern,'Добавить CellPattern','cell_pattern'],
 [Checkerboard,'Добавить Checkerboard','checkerboard'],
 [LensFlare,'Добавить Lens Flare','lens_flare'],
 [Grid,'Добавить Grid','grid'],
 [Circle,'Добавить Circle','circle'],
 [Ramp,'Добавить Ramp','ramp'],
 [FourColorGradient,'Добавить 4-Color Gradient','four_color_gradient'],
 [BevelAlpha,'Добавить Bevel Alpha','bevel_alpha'],
 [DropShadow,'Добавить Drop Shadow','drop_shadow'],
 [Basic3D,'Добавить Basic 3D','basic_3d'],
 [LensDistortion,'Добавить Lens Distortion','lens_distortion'],
 [WaveWarp,'Добавить Wave Warp','wave_warp'],
 [Twirl,'Добавить Twirl','twirl'],
 [TwirlLegacy,'Добавить Twirl Legacy','twirl_legacy'],
 [Emboss,'Добавить Emboss','emboss'],
 [FindEdges,'Добавить Find Edges','find_edges'],
 [Mosaic,'Добавить Mosaic','mosaic'],
 [MosaicLegacy,'Добавить Mosaic Legacy','mosaic_legacy'],
 [Median,'Добавить Median','median'],
 [CameraBlur,'Добавить Camera Blur','camera_blur'],
 [GaussianBlur,'Добавить Gaussian Blur','gaussian_blur'],
 [GaussianBlurLegacy,'Добавить Gaussian Blur Legacy','gaussian_blur_legacy'],
 [UltraKey,'Добавить Ultra Key','ultra_key'],
 [ColorKey,'Добавить Color Key','color_key'],
 [LumaKey,'Добавить Luma Key','luma_key'],
 [AlphaAdjust,'Добавить Alpha Adjust','alpha_adjust'],
 [ChannelMix,'Добавить Channel Mix','channel_mix'],
 [ColorReplace,'Добавить Color Replace','color_replace'],
 [ColorPass,'Добавить Color Pass','color_pass'],
 [ChangeToColor,'Добавить Change to Color','change_to_color'],
 [LeaveColor,'Добавить Leave Color','leave_color'],
 [Balance,'Добавить Color Balance','color_balance'],
 [Tint,'Добавить Tint','color_tint'],
 [BlackWhite,'Добавить Black & White','black_white'],
 [HorizontalFlip,'Добавить Horizontal Flip','horizontal_flip'],
 [VerticalFlip,'Добавить Vertical Flip','vertical_flip'],
 [Extract,'Добавить Extract','color_extract'],
 [Basic,'Добавить Базовая коррекция','basic_grade'],
 [Sharpen,'Добавить Резкость','color_sharpen'],
 [Chroma,'Добавить chroma matte','chroma_matte'],
 [Lumetri,'Добавить Lumetri','lumetri'],
 [Vignette,'Добавить Виньетка','color_vignette'],
 [Hsl,'Добавить HSL Secondary','hsl_secondary'],
 [Creative,'Добавить колёса и образ','creative_grade'],
 [Adjust,'Добавить Творческая коррекция','creative_adjust'],
 [Curves,'Добавить RGB-кривые','tone_curves'],
 [Masks,'Прямоугольник','mask'],
 [Masks,'Эллипс','mask'],
 [Masks,'Линейная','linear_mask'],
] as const)('authors image effects through %s',async (Inspector,label,kind) => {
 component=mount(Inspector,{target:document.body});await tick()
 const button=[...document.querySelectorAll('button')].find(button=>button.textContent?.trim()===label)!
 expect(button).toBeDefined();expect(button.disabled).toBe(false);button.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0].kind).toBe(kind)
 undoNestedComposition();await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects).toBeUndefined()
 redoNestedComposition();await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
 expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 if(kind==='lumetri'){await tick();expect(listBuiltinLuts).toHaveBeenCalledOnce()}
})

it('removes Black & White while preserving other effects and restores exact order',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'gamma_correction',parameters:{gamma:20}},{kind:'black_white'},{kind:'posterize',parameters:{levels:7}}]
 nestedComposition.document=project
 component=mount(BlackWhite,{target:document.body});await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Black & White 1 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects).toEqual([project.compositions.root.content.tracks[0].clips[0].effects[0],project.compositions.root.content.tracks[0].clips[0].effects[2]])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(project)
 redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('authors Extract image softness keys and inversion independently',async()=>{
 component=mount(Extract,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(button=>button.textContent?.trim()==='Добавить Extract')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const softness=document.querySelector<HTMLInputElement>('input[aria-label="photo Extract softness"]')!
 softness.value='25';softness.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Extract softness key"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 const invert=document.querySelector<HTMLInputElement>('input[aria-label="photo Extract invert"]')!
 invert.checked=true;invert.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({kind:'color_extract',parameters:{softness:25,invert:true},animation:{softness:{track:{keyframes:[{tick:500000,value:25}]}}}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('edits image mask geometry and restores removed mask through undo',async()=>{
 component=mount(Masks,{target:document.body});await tick()
 const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Эллипс')!
 button.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const width=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 width"]')!
 width.value='0.5';width.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 const inverted=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 inverted"]')!
 inverted.checked=true;inverted.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 width key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({shape:'ellipse',width:{mode:'keyframes',track:{keyframes:[{tick:500000,value:0.5}]}},inverted:true})
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Удалить маску')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('edits mask key time and value, removes keys and clears curves with undo',async()=>{
 component=mount(Masks,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Линейная')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 async function change(label:string,value:string,count:number){
  const input=document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!
  input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
 }
 await change('photo mask 0 x 500000 value','0.25',3)
 await change('photo mask 0 x 500000 time','0.75',4)
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0].x.track.keyframes).toEqual([{tick:750000,value:0.25}])
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x 750000 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x).toEqual({mode:'constant',value:0.25})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x clear"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.future).toHaveLength(0));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x).toEqual({mode:'constant',value:0.25})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})

it('reorders whole effect objects and preserves animations through undo and deletion',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 const effects=[{kind:'gamma_correction',parameters:{gamma:20},animation:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:10},{tick:500000,value:20}]}}},{kind:'proc_amp',parameters:{brightness:10}}]
 project.compositions.root.content.tracks[0].clips[0].effects=effects
 nestedComposition.document=project
 component=mount(Stack,{target:document.body});await tick()
 expect(document.querySelector<HTMLButtonElement>('button[aria-label="photo effect 0 up"]')!.disabled).toBe(true)
 expect(document.querySelector<HTMLButtonElement>('button[aria-label="photo effect 1 down"]')!.disabled).toBe(true)
 document.querySelector<HTMLButtonElement>('button[aria-label="photo effect 0 down"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects).toEqual([effects[1],effects[0]])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(project)
 redoNestedComposition();await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo effect 0 remove"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects).toEqual([effects[0]])
 undoNestedComposition();await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects).toEqual([effects[1],effects[0]])
})

it('shows the sampled mask value and inserts a key without changing the interpolated curve',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:1000000,value:1}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;nestedComposition.timeUs=600000
 component=mount(Masks,{target:document.body});await tick()
 expect(document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x"]')!.value).toBe('0.5')
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x.track.keyframes).toEqual([{tick:0,value:0},{tick:500000,value:0.5},{tick:1000000,value:1}])
 nestedComposition.timeUs=850000;await tick()
 expect(document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x"]')!.value).toBe('0.75')
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x clear"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x).toEqual({mode:'constant',value:0.75})
 undoNestedComposition();await tick()
 expect(document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x"]')!.value).toBe('0.75')
})
it('keeps image mask history and cached output intact after native rejection',async()=>{
 nestedComposition.renderedOutputs={root:'existing-render'}
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('invalid mask geometry'))
 component=mount(Masks,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Эллипс')!.click()
 await vi.waitFor(()=>expect(nestedHistory.error).toBe('invalid mask geometry'));await tick()
 expect(nestedHistory.busy).toBe(false)
 expect(nestedHistory.past).toEqual([]);expect(nestedHistory.future).toEqual([])
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 expect(nestedComposition.renderedOutputs).toEqual({root:'existing-render'})
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Эллипс')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(nestedHistory.error).toBe('');expect(nestedComposition.renderedOutputs).toEqual({})
})
it('disables effect stack edits on locked image tracks',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].locked=true
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'gamma_correction',parameters:{gamma:20}},{kind:'proc_amp',parameters:{brightness:10}}]
 nestedComposition.document=project
 component=mount(Stack,{target:document.body});await tick()
 const buttons=[...document.querySelectorAll<HTMLButtonElement>('button')]
 expect(buttons).toHaveLength(8)
 for(const button of buttons){expect(button.disabled).toBe(true);button.click()}
 await tick()
 expect(evaluateNestedFrame).not.toHaveBeenCalled()
 expect(nestedHistory.past).toEqual([])
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(project)
})
it('changes mask interpolation through native history and updates cursor sampling',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:1000000,value:1}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;nestedComposition.timeUs=350000
 component=mount(Masks,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x"]')!
 expect(input.value).toBe('0.25')
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo mask 0 x interpolation"]')!
 expect(select.value).toBe('linear')
 select.value='ease_in';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(input.value).toBe('0.015625')
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x.track.keyframes).toEqual(project.compositions.root.content.tracks[0].clips[0].effects[0].x.track.keyframes)
 undoNestedComposition();await tick();expect(input.value).toBe('0.25');expect(select.value).toBe('linear')
 redoNestedComposition();await tick();expect(input.value).toBe('0.015625');expect(select.value).toBe('ease_in')
})
it('authors custom mask Bezier and Elastic parameters with undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:1000000,value:1}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;nestedComposition.timeUs=350000
 component=mount(Masks,{target:document.body});await tick()
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo mask 0 x interpolation"]')!
 select.value='bezier';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const x=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x p1 0"]')!
 x.value='0.4';x.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 const current=()=>JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x.track
 expect(current().interpolation).toEqual({kind:'bezier',p1:[0.4,0.1],p2:[0.25,1]})
 const bezier=JSON.parse(JSON.stringify(nestedComposition.document))
 select.value='elastic';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const period=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x period"]')!
 period.value='0.5';period.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 expect(current().interpolation).toEqual({kind:'elastic',dir:'out',amplitude:1,period:0.5})
 expect(current().keyframes).toEqual(project.compositions.root.content.tracks[0].clips[0].effects[0].x.track.keyframes)
 undoNestedComposition();undoNestedComposition();await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(bezier)
 expect(select.value).toBe('bezier')
})
it('authors independent mask extrapolation sides and samples repeated motion',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0.25},{tick:1000000,value:0.75}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;nestedComposition.timeUs=1350000
 component=mount(Masks,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x"]')!
 expect(input.value).toBe('0.75')
 const after=document.querySelector<HTMLSelectElement>('select[aria-label="photo mask 0 x after"]')!
 after.value='loop';after.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(input.value).toBe('0.375')
 const before=document.querySelector<HTMLSelectElement>('select[aria-label="photo mask 0 x before"]')!
 before.value='continue';before.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x.track.extrapolation).toEqual({before:'continue',after:'loop'})
 undoNestedComposition();await tick();expect(before.value).toBe('hold');expect(after.value).toBe('loop')
 undoNestedComposition();await tick();expect(input.value).toBe('0.75');expect(after.value).toBe('hold')
})
it('sets and clears segment interpolation without changing other keys or channel default',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:500000,value:0.5},{tick:1000000,value:1}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;nestedComposition.timeUs=350000
 component=mount(Masks,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x"]')!
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo mask 0 x 0 interpolation"]')!
 expect(input.value).toBe('0.25');expect(select.value).toBe('inherit')
 select.value='hold';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(input.value).toBe('0')
 const track=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x.track
 expect(track.interpolation).toBe('linear');expect(track.keyframes).toEqual([{tick:0,value:0,interpolation:'hold'},{tick:500000,value:0.5},{tick:1000000,value:1}])
 select.value='inherit';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(input.value).toBe('0.25')
 undoNestedComposition();await tick();expect(input.value).toBe('0');expect(select.value).toBe('hold')
})
it('authors mask tangent handles and preserves them through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:{kind:'bezier',p1:[0.25,0.1],p2:[0.25,1]},keyframes:[{tick:0,value:0},{tick:1000000,value:1}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;component=mount(Masks,{target:document.body});await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x 0 tangents add"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x 0 outgoing x"]')!
 expect(input.value).toBe('0.25');input.value='0.4';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0].x.track.keyframes[0].tangents.outgoing).toEqual({x:0.4,y:0.1,mode:'free'})
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x 0 tangents clear"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('normalizes automatic mask tangents and restores free coordinates through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:{kind:'bezier',p1:[0.25,0.1],p2:[0.25,1]},keyframes:[{tick:0,value:0},{tick:1000000,value:1}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;component=mount(Masks,{target:document.body});await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo mask 0 x 0 tangents add"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 const mode=document.querySelector<HTMLSelectElement>('select[aria-label="photo mask 0 x 0 outgoing mode"]')!
 mode.value='auto';mode.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x.track.keyframes[0].tangents.outgoing).toEqual({x:1/3,y:0,mode:'auto'})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved);expect(mode.value).toBe('free')
 redoNestedComposition();await tick();expect(mode.value).toBe('auto')
})
it('edits custom segment curve parameters without replacing the channel curve',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_mask',x:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:1000000,value:1}]}},y:{mode:'constant',value:0.5},rotationDegrees:{mode:'constant',value:0},feather:0,inverted:false}]
 nestedComposition.document=project;component=mount(Masks,{target:document.body});await tick()
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo mask 0 x 0 interpolation"]')!
 select.value='bezier';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const point=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x 0 curve p2 1"]')!
 point.value='0.8';point.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 const track=()=>JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].x.track
 expect(track().interpolation).toBe('linear');expect(track().keyframes[0].interpolation).toEqual({kind:'bezier',p1:[0.25,0.1],p2:[0.25,0.8]})
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 select.value='elastic';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const period=document.querySelector<HTMLInputElement>('input[aria-label="photo mask 0 x 0 curve period"]')!
 period.value='0.5';period.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 expect(track().keyframes[0].interpolation).toEqual({kind:'elastic',dir:'out',amplitude:1,period:0.5})
 undoNestedComposition();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})

it('authors independent Tint image endpoint keys at clip-local time and restores exact curves',async()=>{
 component=mount(Tint,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Tint')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const red=document.querySelector<HTMLInputElement>('input[aria-label="photo Tint black R"]')!
 red.value='-0.5';red.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Tint blackR ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Tint whiteA ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{black:[-0.5,0,0,1]},colorAnimation:{blackR:{track:{keyframes:[{tick:500000,value:-0.5}]}},whiteA:{track:{keyframes:[{tick:500000,value:1}]}}}})
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Tint blackR удалить анимацию"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const effect=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.colorAnimation.blackR).toBeUndefined();expect(effect.colorAnimation.whiteA).toEqual(saved.compositions.root.content.tracks[0].clips[0].effects[0].colorAnimation.whiteA)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('authors independent Color Balance image keys and preserves curves when toggling luminosity',async()=>{
 component=mount(Balance,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Color Balance')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 let count=1
 for(const [channel,value] of [['shadowR',75],['midG',-50],['hiB',100]] as const){
  const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Color Balance ${channel}"]`)!
  input.value=String(value);input.dispatchEvent(new Event('change',{bubbles:true}))
  count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Color Balance ${channel} key"]`)!.click()
  count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo Color Balance preserve"]')!
 checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}))
 count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toMatchObject({shadowR:75,midG:-50,hiB:100,preserve:true})
 expect(effect.animation).toEqual(before.compositions.root.content.tracks[0].clips[0].effects[0].animation)
 for(const [channel,value] of [['shadowR',75],['midG',-50],['hiB',100]] as const)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('authors and edits Leave Color image color keys independently of scalar keys with exact undo',async()=>{
 component=mount(LeaveColor,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Leave Color')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Leave Color amount key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Leave Color color R key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Leave Color color A key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Leave Color color R 500000 value"]')!
 input.value='0.25';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({animation:{amount:{track:{keyframes:[{tick:500000,value:0}]}}},colorAnimation:{r:{track:{keyframes:[{tick:500000,value:0.25}]}},a:{track:{keyframes:[{tick:500000,value:1}]}}}})
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Leave Color color R 500000 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const effect=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.colorAnimation.r).toBeUndefined();expect(effect.colorAnimation.a).toEqual(saved.compositions.root.content.tracks[0].clips[0].effects[0].colorAnimation.a)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('authors independent Change to Color source destination and scalar local keys with exact undo',async()=>{
 component=mount(ChangeToColor,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Change to Color')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Change to Color hueTol key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Change to Color fromR ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Change to Color toB ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Change to Color toB 500000 value"]')!
 input.value='0.5';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({animation:{hueTol:{track:{keyframes:[{tick:500000,value:5}]}}},colorAnimation:{fromR:{track:{keyframes:[{tick:500000,value:1}]}},toB:{track:{keyframes:[{tick:500000,value:0.5}]}}}})
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Change to Color toB 500000 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const effect=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.colorAnimation.toB).toBeUndefined();expect(effect.colorAnimation.fromR).toEqual(saved.compositions.root.content.tracks[0].clips[0].effects[0].colorAnimation.fromR)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('preserves Color Pass similarity and color keys when toggling reverse with exact undo',async()=>{
 component=mount(ColorPass,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Color Pass')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Pass similarity key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Pass color G key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Color Pass color G 500000 value"]')!
 input.value='0.25';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 const reverse=document.querySelector<HTMLInputElement>('input[aria-label="photo Color Pass reverse"]')!
 reverse.checked=true;reverse.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document)),effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect).toMatchObject({parameters:{reverse:true},animation:{track:{keyframes:[{tick:500000,value:10}]}},colorAnimation:{g:{track:{keyframes:[{tick:500000,value:0.25}]}}}})
 expect(effect.animation).toEqual(before.compositions.root.content.tracks[0].clips[0].effects[0].animation)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('authors independent Color Replace source destination and scalar local keys with exact undo',async()=>{
 component=mount(ColorReplace,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Color Replace')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Replace similarity key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Replace targetR ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Replace replaceB ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Color Replace replaceB 500000 value"]')!
 input.value='0.5';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({animation:{track:{keyframes:[{tick:500000,value:10}]}},colorAnimation:{targetR:{track:{keyframes:[{tick:500000,value:1}]}},replaceB:{track:{keyframes:[{tick:500000,value:0.5}]}}}})
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Replace replaceB 500000 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const effect=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.colorAnimation.replaceB).toBeUndefined();expect(effect.colorAnimation.targetR).toEqual(saved.compositions.root.content.tracks[0].clips[0].effects[0].colorAnimation.targetR)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
 const solid=document.querySelector<HTMLInputElement>('input[aria-label="photo Color Replace solid"]')!
 solid.checked=true;solid.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const solidSaved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(solidSaved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({...saved.compositions.root.content.tracks[0].clips[0].effects[0],parameters:{solid:true}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
 redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(solidSaved)
})
it('authors independent Channel Mix image keys and monochromes curves when toggling monochrome',async()=>{
 component=mount(ChannelMix,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Channel Mix')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 let count=1
 for(const [channel,value] of [['rr',75],['gc',-50],['bb',100]] as const){
  const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Channel Mix ${channel}"]`)!
  input.value=String(value);input.dispatchEvent(new Event('change',{bubbles:true}))
  count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Channel Mix ${channel} key"]`)!.click()
  count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo Channel Mix monochrome"]')!
 checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}))
 count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toMatchObject({rr:75,gc:-50,bb:100,monochrome:true})
 expect(effect.animation).toEqual(before.compositions.root.content.tracks[0].clips[0].effects[0].animation)
 for(const [channel,value] of [['rr',75],['gc',-50],['bb',100]] as const)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})

it('preserves Alpha Adjust opacity keys across all flag toggles with exact undo redo',async()=>{
 component=mount(AlphaAdjust,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Alpha Adjust')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Alpha Adjust opacity"]')!
 input.value='50';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Alpha Adjust opacity key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 let count=3
 for(const flag of ['ignore','invert','maskOnly']){
  const checkbox=document.querySelector<HTMLInputElement>(`input[aria-label="photo Alpha Adjust ${flag}"]`)!
  checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}))
  count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
 }
 const saved=JSON.parse(JSON.stringify(nestedComposition.document)),effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toEqual({opacity:50,ignore:true,invert:true,maskOnly:true})
 expect(effect.animation).toEqual(before.compositions.root.content.tracks[0].clips[0].effects[0].animation)
 expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:50}])
 for(let i=0;i<3;i++){undoNestedComposition();await tick()}
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 for(let i=0;i<3;i++){redoNestedComposition();await tick()}
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})

it('edits Luma Key independent threshold cutoff local curves including crossed values and exact undo',async()=>{
 component=mount(LumaKey,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Luma Key')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 let count=1
 for(const [channel,value] of [['threshold',50],['cutoff',75]] as const){
  const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Luma Key ${channel}"]`)!
  input.value=String(value);input.dispatchEvent(new Event('change',{bubbles:true}))
  count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Luma Key ${channel} key"]`)!.click()
  count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
 }
 const edited=document.querySelector<HTMLInputElement>('input[aria-label="photo Luma Key cutoff 500000 value"]')!
 edited.value='25';edited.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document)),effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toEqual({threshold:50,cutoff:75})
 expect(effect.animation.threshold.track.keyframes).toEqual([{tick:500000,value:50}])
 expect(effect.animation.cutoff.track.keyframes).toEqual([{tick:500000,value:25}])
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Luma Key cutoff 500000 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 const removed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]
 expect(removed.animation.cutoff).toBeUndefined();expect(removed.animation.threshold).toEqual(effect.animation.threshold)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})
it('preserves Color Key tolerance and color keys when toggling reverse with exact undo',async()=>{
 component=mount(ColorKey,{target:document.body});await tick()
 ;[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить Color Key')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Key tolerance key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Color Key color G key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Color Key color G 500000 value"]')!
 input.value='0.25';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 const thin=document.querySelector<HTMLInputElement>('input[aria-label="photo Color Key thin"]')!
 thin.value='-5';thin.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document)),effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect).toMatchObject({parameters:{thin:-5},animation:{tolerance:{track:{keyframes:[{tick:500000,value:0}]}}},colorAnimation:{g:{track:{keyframes:[{tick:500000,value:0.25}]}}}})
 expect(effect.animation).toEqual(before.compositions.root.content.tracks[0].clips[0].effects[0].animation)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
})


it('authors Twirl center keys from source dimensions and preserves exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.sources={source:{id:'source',width:18,height:10}}
 const clip=project.compositions.root.content.tracks[0].clips[0]
 clip.sourceId='source';clip.effects=[{kind:'twirl',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=350000
 component=mount(Twirl,{target:document.body});await tick()
 const center=document.querySelector<HTMLInputElement>('input[aria-label="photo Twirl centerX"]')!
 expect(center.value).toBe('')
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Twirl centerX key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const keyed=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(keyed.compositions.root.content.tracks[0].clips[0].effects[0].animation.centerX.track.keyframes).toEqual([{tick:250000,value:9}])
 center.value='1.23456789012345';center.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.center).toEqual([1.23456789012345,null])
 const explicit=JSON.parse(JSON.stringify(nestedComposition.document))
 center.value='';center.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.center).toEqual([null,null])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(explicit)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(keyed)
})

it('authors Drop Shadow only mode preserving its color and restores exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'drop_shadow',parameters:{color:[1,0.5,0,0],softness:20,only:false}}]
 nestedComposition.document=project
 component=mount(DropShadow,{target:document.body});await tick()
 const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo DropShadow only"]')!
 checkbox.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters).toEqual({color:[1,0.5,0,0],softness:20,only:true})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(project)
})
it('authors Gradient Wipe curves placement and inversion independently with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'gradient_wipe',parameters:{completion:50,softness:25,layer:1,placement:2,invert:false}}]
 nestedComposition.document=project;component=mount(GradientWipe,{target:document.body});await tick()
 nestedComposition.timeUs=600000;await tick()
 for(const [i,channel] of ['completion','softness'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo GradientWipe ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(before.compositions.root.content.tracks[0].clips[0].effects[0].animation).toMatchObject({completion:{track:{keyframes:[{tick:500000,value:50}]}},softness:{track:{keyframes:[{tick:500000,value:25}]}}})
 const placement=document.querySelector<HTMLSelectElement>('select[aria-label="photo GradientWipe placement"]')!;placement.value='0';placement.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const afterPlacement=JSON.parse(JSON.stringify(nestedComposition.document));expect(afterPlacement.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{completion:50,softness:25,layer:1,placement:0,invert:false},animation:before.compositions.root.content.tracks[0].clips[0].effects[0].animation})
 document.querySelector<HTMLInputElement>('input[aria-label="photo GradientWipe invert"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.invert).toBe(true)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(afterPlacement)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})
it('authors Linear Wipe three local curves and preserves them on base angle edit and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'linear_wipe',parameters:{completion:50,angle:135,feather:25}}];nestedComposition.document=project;component=mount(LinearWipe,{target:document.body});await tick();nestedComposition.timeUs=600000;await tick()
 for(const[i,channel]of ['completion','angle','feather'].entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo LinearWipe ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));expect(before.compositions.root.content.tracks[0].clips[0].effects[0].animation).toMatchObject({completion:{track:{keyframes:[{tick:500000,value:50}]}},angle:{track:{keyframes:[{tick:500000,value:135}]}},feather:{track:{keyframes:[{tick:500000,value:25}]}}})
 const angle=document.querySelector<HTMLInputElement>('input[aria-label="photo LinearWipe angle"]')!;angle.value='720';angle.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick();const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{completion:50,angle:720,feather:25},animation:before.compositions.root.content.tracks[0].clips[0].effects[0].animation});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})
it('authors six Magnify local curves preserves auto center and exact undo across shape and blend edits',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={source:{id:'source',width:18,height:10}};const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind:'magnify',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=350000;component=mount(Magnify,{target:document.body});await tick()
 for(const[i,channel]of ['centerX','centerY','magnification','size','feather','opacity'].entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Magnify ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const keyed=JSON.parse(JSON.stringify(nestedComposition.document));const effect=keyed.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries({centerX:9,centerY:5,magnification:200,size:100,feather:0,opacity:100})){expect(effect.animation[channel].track.keyframes).toEqual([{tick:250000,value}])}expect(effect.parameters.center).toBeUndefined()
 const center=document.querySelector<HTMLInputElement>('input[aria-label="photo Magnify centerX"]')!;center.value='1.23456789012345';center.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();const explicit=JSON.parse(JSON.stringify(nestedComposition.document));expect(explicit.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{center:[1.23456789012345,null]},animation:effect.animation})
 for(const[i,channel]of ['shape','mode'].entries()){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo Magnify ${channel}"]`)!;select.value=channel==='shape'?'1':'4';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(8+i));await tick()}
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{center:[1.23456789012345,null],shape:1,mode:4},animation:effect.animation})
 undoNestedComposition();await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(explicit);undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(keyed)
})

it('authors Spherize center keys from source dimensions and preserves exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.sources={source:{id:'source',width:18,height:10}}
 const clip=project.compositions.root.content.tracks[0].clips[0]
 clip.sourceId='source';clip.effects=[{kind:'spherize',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=350000
 component=mount(Spherize,{target:document.body});await tick()
 const center=document.querySelector<HTMLInputElement>('input[aria-label="photo Spherize centerX"]')!
 expect(center.value).toBe('')
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Spherize centerX key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const keyed=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(keyed.compositions.root.content.tracks[0].clips[0].effects[0].animation.centerX.track.keyframes).toEqual([{tick:250000,value:9}])
 center.value='1.23456789012345';center.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.center).toEqual([1.23456789012345,null])
 const explicit=JSON.parse(JSON.stringify(nestedComposition.document))
 center.value='';center.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.center).toEqual([null,null])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(explicit)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(keyed)
})

it('authors all three Spherize curves and preserves them through radius base edit and exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={source:{id:'source',width:18,height:10}};const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind:'spherize',parameters:{radius:2.5}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Spherize,{target:document.body});await tick()
 for(const[i,channel]of ['radius','centerX','centerY'].entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Spherize ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const e=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(e.animation).toMatchObject({radius:{track:{keyframes:[{tick:500000,value:2.5}]}},centerX:{track:{keyframes:[{tick:500000,value:9}]}},centerY:{track:{keyframes:[{tick:500000,value:5}]}}});expect(e.parameters.center).toBeUndefined()
 const radius=document.querySelector<HTMLInputElement>('input[aria-label="photo Spherize radius"]')!;radius.value='100';radius.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{radius:100},animation:e.animation});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors Mirror center keys from source dimensions and preserves exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.sources={source:{id:'source',width:18,height:10}}
 const clip=project.compositions.root.content.tracks[0].clips[0]
 clip.sourceId='source';clip.effects=[{kind:'mirror',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=350000
 component=mount(Mirror,{target:document.body});await tick()
 const center=document.querySelector<HTMLInputElement>('input[aria-label="photo Mirror centerX"]')!
 expect(center.value).toBe('')
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Mirror centerX key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const keyed=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(keyed.compositions.root.content.tracks[0].clips[0].effects[0].animation.centerX.track.keyframes).toEqual([{tick:250000,value:9}])
 center.value='1.23456789012345';center.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.center).toEqual([1.23456789012345,null])
 const explicit=JSON.parse(JSON.stringify(nestedComposition.document))
 center.value='';center.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.center).toEqual([null,null])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(explicit)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(keyed)
})

it('authors all three Mirror curves and preserves them through angle base edit and exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={source:{id:'source',width:18,height:10}};const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind:'mirror',parameters:{angle:2.5}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Mirror,{target:document.body});await tick()
 for(const[i,channel]of ['angle','centerX','centerY'].entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Mirror ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const e=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(e.animation).toMatchObject({angle:{track:{keyframes:[{tick:500000,value:2.5}]}},centerX:{track:{keyframes:[{tick:500000,value:9}]}},centerY:{track:{keyframes:[{tick:500000,value:5}]}}});expect(e.parameters.center).toBeUndefined()
 const angle=document.querySelector<HTMLInputElement>('input[aria-label="photo Mirror angle"]')!;angle.value='100';angle.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{angle:100},animation:e.animation});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors Offset shift keys from source dimensions and preserves exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.sources={source:{id:'source',width:18,height:10}}
 const clip=project.compositions.root.content.tracks[0].clips[0]
 clip.sourceId='source';clip.effects=[{kind:'offset',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=350000
 component=mount(Offset,{target:document.body});await tick()
 const shift=document.querySelector<HTMLInputElement>('input[aria-label="photo Offset shiftX"]')!
 expect(shift.value).toBe('')
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Offset shiftX key"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const keyed=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(keyed.compositions.root.content.tracks[0].clips[0].effects[0].animation.shiftX.track.keyframes).toEqual([{tick:250000,value:9}])
 shift.value='1.23456789012345';shift.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.shift).toEqual([1.23456789012345,null])
 const explicit=JSON.parse(JSON.stringify(nestedComposition.document))
 shift.value='';shift.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.shift).toEqual([null,null])
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(explicit)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(keyed)
})

it('authors all three Offset curves and preserves them through blend base edit and exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={source:{id:'source',width:18,height:10}};const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind:'offset',parameters:{blend:2.5}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Offset,{target:document.body});await tick()
 for(const[i,channel]of ['blend','shiftX','shiftY'].entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Offset ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const e=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(e.animation).toMatchObject({blend:{track:{keyframes:[{tick:500000,value:2.5}]}},shiftX:{track:{keyframes:[{tick:500000,value:9}]}},shiftY:{track:{keyframes:[{tick:500000,value:5}]}}});expect(e.parameters.shift).toBeUndefined()
 const blend=document.querySelector<HTMLInputElement>('input[aria-label="photo Offset blend"]')!;blend.value='100';blend.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({parameters:{blend:100},animation:e.animation});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors eight Corner Pin keys from source corners and preserves precise base edits with undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 project.compositions.root.content.sources={source:{id:'source',width:18,height:10}}
 const clip=project.compositions.root.content.tracks[0].clips[0]
 clip.sourceId='source';clip.effects=[{kind:'corner_pin',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000
 component=mount(CornerPin,{target:document.body});await tick()
 const channels=['upperLeftX','upperLeftY','upperRightX','upperRightY','lowerLeftX','lowerLeftY','lowerRightX','lowerRightY']
 const defaults=[0,0,18,0,0,10,18,10]
 for(const[i,channel]of channels.entries()){
  expect(document.querySelector<HTMLInputElement>(`input[aria-label="photo CornerPin ${channel}"]`)!.value).toBe('')
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo CornerPin ${channel} key"]`)!.click()
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 const effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[i,channel]of channels.entries())expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:defaults[i]}])
 expect(effect.parameters).toEqual({})
 const x=document.querySelector<HTMLInputElement>('input[aria-label="photo CornerPin upperLeftX"]')!
 x.value='1.23456789012345';x.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick()
 const edited=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(edited.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{upperLeft:[1.23456789012345,null]}})
 x.value='';x.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(10));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{upperLeft:[null,null]}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(edited)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors five 3D Rotate curves and preserves them through base and back-face edits with undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document))
 const clip=project.compositions.root.content.tracks[0].clips[0]
 clip.effects=[{kind:'rotate_3d',parameters:{rotX:30,rotY:45,rotZ:-15,perspective:50,z:-100}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000
 component=mount(Rotate3D,{target:document.body});await tick()
 for(const[i,channel]of ['rotX','rotY','rotZ','perspective','z'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Rotate3D ${channel} key"]`)!.click()
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document))
 const effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries(effect.parameters))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const angle=document.querySelector<HTMLInputElement>('input[aria-label="photo Rotate3D rotX"]')!
 angle.value='180';angle.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,rotX:180}})
 const hide=document.querySelector<HTMLInputElement>('input[aria-label="photo Rotate3D hideBack"]')!
 hide.checked=true;hide.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,rotX:180,hideBack:true}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

for(const [kind,Inspector,name,from,to]of [['grow',Grow,'Grow',100,120],['shrink',Shrink,'Shrink',120,100]] as const){
 it(`authors four ${name} keys with correct defaults and preserves easing/base edits through exact undo`,async()=>{
  const project=JSON.parse(JSON.stringify(nestedComposition.document))
  project.compositions.root.content.sources={source:{id:'source',width:18,height:10}}
  const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind,parameters:{}}]
  nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Inspector,{target:document.body});await tick()
  for(const[i,channel]of ['from','to','centerX','centerY'].entries()){
   document.querySelector<HTMLButtonElement>(`button[aria-label="photo ${name} ${channel} key"]`)!.click()
   await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
  }
  const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
  for(const[channel,value]of Object.entries({from,to,centerX:9,centerY:5}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
  const center=document.querySelector<HTMLInputElement>(`input[aria-label="photo ${name} centerX"]`)!
  center.value='1.23456789012345';center.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
  const changed=JSON.parse(JSON.stringify(nestedComposition.document))
  expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{center:[1.23456789012345,null]}})
  const easing=document.querySelector<HTMLSelectElement>(`select[aria-label="photo ${name} easing"]`)!
  easing.value='3';easing.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
  expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{center:[1.23456789012345,null],easing:3}})
  undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
  undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 })
}

it('authors four Spin keys with automatic center and preserves easing/base edits with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={source:{id:'source',width:18,height:10}}
 const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind:'spin',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Spin,{target:document.body});await tick()
 for(const[i,channel]of ['amount','scale','centerX','centerY'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Spin ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({amount:360,scale:100,centerX:9,centerY:5}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const center=document.querySelector<HTMLInputElement>('input[aria-label="photo Spin centerX"]')!
 center.value='1.23456789012345';center.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{center:[1.23456789012345,null]}})
 const easing=document.querySelector<HTMLSelectElement>('select[aria-label="photo Spin easing"]')!;expect(easing.value).toBe('3')
 easing.value='1';easing.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{center:[1.23456789012345,null],easing:1}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors five Move keys and preserves precise points and easing through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const clip=project.compositions.root.content.tracks[0].clips[0];clip.effects=[{kind:'move',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Move,{target:document.body});await tick()
 for(const[i,channel]of ['fromX','fromY','toX','toY','motionBlur'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Move ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({fromX:-100,fromY:0,toX:0,toY:0,motionBlur:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const x=document.querySelector<HTMLInputElement>('input[aria-label="photo Move fromX"]')!;x.value='1.23456789012345';x.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{from:[1.23456789012345,0]}})
 const easing=document.querySelector<HTMLSelectElement>('select[aria-label="photo Move easing"]')!;expect(easing.value).toBe('3');easing.value='1';easing.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{from:[1.23456789012345,0],easing:1}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors five Wiggle keys and preserves base values and dimension choices through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const clip=project.compositions.root.content.tracks[0].clips[0];clip.effects=[{kind:'wiggle',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Wiggle,{target:document.body});await tick()
 for(const[i,channel]of ['frequency','amount','rotation','scale','seed'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Wiggle ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({frequency:2,amount:20,rotation:0,scale:0,seed:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const x=document.querySelector<HTMLInputElement>('input[aria-label="photo Wiggle seed"]')!;x.value='42.75';x.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{seed:42.75}})
 const dimensions=document.querySelector<HTMLSelectElement>('select[aria-label="photo Wiggle dimensions"]')!;expect(dimensions.value).toBe('0');dimensions.value='1';dimensions.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{seed:42.75,dimensions:1}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors seven CameraShake keys and preserves base values and motion blur through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const clip=project.compositions.root.content.tracks[0].clips[0];clip.effects=[{kind:'camera_shake',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(CameraShake,{target:document.body});await tick()
 for(const[i,channel]of ['frequency','amount','rotation','zoom','complexity','motionBlur','seed'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo CameraShake ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({frequency:4,amount:20,rotation:1,zoom:5,complexity:3,motionBlur:0,seed:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const x=document.querySelector<HTMLInputElement>('input[aria-label="photo CameraShake seed"]')!;x.value='42.75';x.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(8));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{seed:42.75}})
 const blur=document.querySelector<HTMLInputElement>('input[aria-label="photo CameraShake motionBlur"]')!;expect(blur.value).toBe('0');blur.value='50';blur.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{seed:42.75,motionBlur:50}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors nine Spacer keys preserving color companions and uniform/fill flags with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'spacer',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Spacer,{target:document.body});await tick()
 for(const[i,channel]of ['left','top','right','bottom','radius','colorR','colorG','colorB','colorA'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Spacer ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({left:40,top:40,right:40,bottom:40,radius:0,colorR:0,colorG:0,colorB:0,colorA:1}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const color=document.querySelector<HTMLInputElement>('input[aria-label="photo Spacer colorG"]')!;color.value='0.5';color.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(10));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{color:[0,0.5,0,1]}})
 const uniform=document.querySelector<HTMLInputElement>('input[aria-label="photo Spacer uniform"]')!;expect(uniform.checked).toBe(true);uniform.checked=false;uniform.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(11));await tick()
 const separate=JSON.parse(JSON.stringify(nestedComposition.document));expect(separate.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{color:[0,0.5,0,1],uniform:false}})
 const fill=document.querySelector<HTMLInputElement>('input[aria-label="photo Spacer fill"]')!;expect(fill.checked).toBe(false);fill.checked=true;fill.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(12));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{color:[0,0.5,0,1],uniform:false,fill:true}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(separate)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors eleven RoundedCrop keys preserving border color companions and feather/border bases with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'rounded_crop',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(RoundedCrop,{target:document.body});await tick()
 for(const[i,channel]of ['left','top','right','bottom','radius','feather','border','borderR','borderG','borderB','borderA'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo RoundedCrop ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({left:0,top:0,right:0,bottom:0,radius:40,feather:0,border:0,borderR:1,borderG:1,borderB:1,borderA:1}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const set=(channel:string,value:string)=>{const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo RoundedCrop ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))}
 set('borderG','0.5');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(12));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{borderColor:[1,0.5,1,1]}})
 set('feather','0.75');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(13));await tick()
 const feathered=JSON.parse(JSON.stringify(nestedComposition.document));expect(feathered.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{borderColor:[1,0.5,1,1],feather:0.75}})
 set('border','2');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(14));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{borderColor:[1,0.5,1,1],feather:0.75,border:2}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(feathered)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors three Clone keys preserving fractional counts and mirror through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'clone',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Clone,{target:document.body});await tick()
 for(const[i,channel]of ['columns','rows','gap'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo Clone ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({columns:2,rows:1,gap:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Clone columns"]')!;input.value='2.75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{columns:2.75}})
 document.querySelector<HTMLInputElement>('input[aria-label="photo Clone mirror"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{columns:2.75,mirror:true}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors AutoAlign margin keys and restores loaded alignment choices with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'auto_align',parameters:{horizontal:1,vertical:3}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(AutoAlign,{target:document.body});await tick()
 expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo AutoAlign horizontal"]')!.value).toBe('1')
 expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo AutoAlign vertical"]')!.value).toBe('3')
 for(const[i,channel]of ['marginX','marginY'].entries()){
 document.querySelector<HTMLButtonElement>(`button[aria-label="photo AutoAlign ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const channel of ['marginX','marginY'])expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:400}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo AutoAlign marginX"]')!;input.value='0.75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{horizontal:1,vertical:3,marginX:0.75}})
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo AutoAlign horizontal"]')!;select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors eight TurbulentDisplace keys preserving auto offset companions choices and cycle with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const content=project.compositions.root.content;content.sources={source:{width:18,height:10}};content.tracks[0].clips[0].sourceId='source';content.tracks[0].clips[0].effects=[{kind:'turbulent_displace',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(TurbulentDisplace,{target:document.body});await tick()
 for(const[i,channel]of ['amount','size','offsetX','offsetY','complexity','evolution','cycleRevs','seed'].entries()){
 document.querySelector<HTMLButtonElement>(`button[aria-label="photo TurbulentDisplace ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({amount:50,size:100,offsetX:9,offsetY:5,complexity:1,evolution:0,cycleRevs:1,seed:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo TurbulentDisplace offsetX"]')!;input.value='3.123456789012';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{offset:[3.123456789012,null]}})
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo TurbulentDisplace displacement"]')!;expect(select.value).toBe('0');select.value='5';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(10));await tick()
 const selected=JSON.parse(JSON.stringify(nestedComposition.document));expect(selected.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{offset:[3.123456789012,null],displacement:5}})
 document.querySelector<HTMLInputElement>('input[aria-label="photo TurbulentDisplace cycle"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(11));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{offset:[3.123456789012,null],displacement:5,cycle:true}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(selected)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('preserves TurbulentDisplace antialias choice through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'turbulent_displace',parameters:{antialias:1}}]
 nestedComposition.document=project;component=mount(TurbulentDisplace,{target:document.body});await tick();const before=JSON.parse(JSON.stringify(nestedComposition.document));const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo TurbulentDisplace antialias"]')!;expect(select.value).toBe('1');select.value='0';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters).toEqual({antialias:0});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors four ChannelBlur keys preserving independent channel bases and repeated edges through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'channel_blur',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(ChannelBlur,{target:document.body});await tick()
 for(const[i,channel]of ['red','green','blue','alpha'].entries()){
  document.querySelector<HTMLButtonElement>(`button[aria-label="photo ChannelBlur ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries({red:0,green:0,blue:0,alpha:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo ChannelBlur red"]')!;input.value='2.75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));expect(changed.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{red:2.75}})
 document.querySelector<HTMLInputElement>('input[aria-label="photo ChannelBlur repeatEdge"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{red:2.75,repeatEdge:true}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(changed)
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})


it('authors ReduceInterlaceFlicker local softness key preserving curve during base edits and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'reduce_interlace_flicker',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(ReduceInterlaceFlicker,{target:document.body});await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Reduce Interlace Flicker softness ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:0}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Reduce Interlace Flicker softness"]')!;input.value='0.31';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{softness:0.31}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors six FocusBlur keys and preserves precise auto-axis center and flags through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const content=project.compositions.root.content;content.sources={source:{width:18,height:10}};content.tracks[0].clips[0].sourceId='source';content.tracks[0].clips[0].effects=[{kind:'focus_blur',parameters:{shape:1}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(FocusBlur,{target:document.body});await tick()
 expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo FocusBlur shape"]')!.value).toBe('1')
 for(const [i,channel] of ['amount','size','feather','angle','centerX','centerY'].entries()){
 document.querySelector<HTMLButtonElement>(`button[aria-label="photo FocusBlur ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const [channel,value] of Object.entries({amount:20,size:200,feather:200,angle:0,centerX:9,centerY:5})) expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo FocusBlur centerX"]')!;input.value='3.123456789012';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.center).toEqual([3.123456789012,null])
 document.querySelector<HTMLInputElement>('input[aria-label="photo FocusBlur show"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(8));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters.show).toBe(true);expect(changed.animation).toEqual(effect.animation)
 undoNestedComposition();await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors four BokehBlur default keys and preserves curves through shape and base edits with undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'bokeh_blur',parameters:{shape:5}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(BokehBlur,{target:document.body});await tick()
 expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo BokehBlur shape"]')!.value).toBe('5')
 for(const [i,channel] of ['amount','rotation','highlight','threshold'].entries()){
 document.querySelector<HTMLButtonElement>(`button[aria-label="photo BokehBlur ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const [channel,value] of Object.entries({amount:10,rotation:0,highlight:50,threshold:80})) expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo BokehBlur amount"]')!;input.value='13.25';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo BokehBlur shape"]')!;select.value='1';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters).toEqual({shape:1,amount:13.25});expect(changed.animation).toEqual(effect.animation)
 undoNestedComposition();await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors Compound Blur local default key and preserves curve through layer and flags with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'compound_blur',parameters:{layer:2,stretch:false,invert:true}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(CompoundBlur,{target:document.body});await tick()
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo Compound Blur layer"]')!;expect(select.value).toBe('2');expect(document.querySelector<HTMLInputElement>('input[aria-label="photo Compound Blur stretch"]')!.checked).toBe(false);expect(document.querySelector<HTMLInputElement>('input[aria-label="photo Compound Blur invert"]')!.checked).toBe(true)
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Compound Blur max ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:20}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Compound Blur max"]')!;input.value='4.25';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 select.value='0';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 document.querySelector<HTMLInputElement>('input[aria-label="photo Compound Blur stretch"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters).toEqual({layer:0,stretch:true,invert:true,max:4.25});expect(changed.animation).toEqual(effect.animation)
 for(let i=0;i<3;i++){undoNestedComposition();await tick()}expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors StandaloneSharpen local amount key preserving curve during base edits and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'sharpen',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(StandaloneSharpen,{target:document.body});await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Sharpen amount ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:0}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Sharpen amount"]')!;input.value='123.45';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{amount:123.45}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors three UnsharpMask default keys and preserves independent curves through base edits and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'unsharp_mask',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(UnsharpMask,{target:document.body});await tick()
 for(const [i,channel] of ['amount','radius','threshold'].entries()){
 document.querySelector<HTMLButtonElement>(`button[aria-label="photo UnsharpMask ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const [channel,value] of Object.entries({amount:50,radius:1,threshold:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 for(const [i,[channel,value]] of Object.entries({amount:123.45,radius:4.25,threshold:32.5}).entries()){
 const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo UnsharpMask ${channel}"]`)!;input.value=String(value);input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+4));await tick()
 }
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters).toEqual({amount:123.45,radius:4.25,threshold:32.5});expect(changed.animation).toEqual(effect.animation)
 for(let i=0;i<3;i++){undoNestedComposition();await tick()}
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors two BrightnessContrast default keys and preserves independent curves through base edits and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'brightness_contrast',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(BrightnessContrast,{target:document.body});await tick()
 for(const [i,channel] of ['brightness','contrast'].entries()){
 document.querySelector<HTMLButtonElement>(`button[aria-label="photo BrightnessContrast ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const [channel,value] of Object.entries({brightness:0,contrast:0}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 for(const [i,[channel,value]] of Object.entries({brightness:-12.5,contrast:37.25}).entries()){
 const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo BrightnessContrast ${channel}"]`)!;input.value=String(value);input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+3));await tick()
 }
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters).toEqual({brightness:-12.5,contrast:37.25});expect(changed.animation).toEqual(effect.animation)
 for(let i=0;i<2;i++){undoNestedComposition();await tick()}
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors Crop five local keys preserving curves through feather and Zoom edits and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'crop',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Crop,{target:document.body});await tick()
 for(const [i,channel] of ['left','top','right','bottom','feather'].entries()){
 document.querySelector<HTMLButtonElement>(`button[aria-label="photo Crop ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()
 }
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const channel of ['left','top','right','bottom','feather'])expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:0}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Crop feather"]')!;input.value='4.25';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 document.querySelector<HTMLInputElement>('input[aria-label="photo Crop zoom"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters).toEqual({feather:4.25,zoom:true});expect(changed.animation).toEqual(effect.animation)
 undoNestedComposition();await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors EdgeFeather local amount key preserving curve during base edits and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'edge_feather',parameters:{}}]
 nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(EdgeFeather,{target:document.body});await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="photo Edge Feather amount ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document)),effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:0}])
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Edge Feather amount"]')!;input.value='12.5';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{amount:12.5}})
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors all ten ASC CDL defaults and preserves curves through fractional base edit and undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'asc_cdl',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(AscCdl,{target:document.body});await tick();
 const channels=["rSlope", "rOffset", "rPower", "gSlope", "gOffset", "gPower", "bSlope", "bOffset", "bPower", "saturation"];
 for(const[i,channel]of channels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo ASC CDL ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const channel of channels)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:channel.endsWith('Offset')?0:1}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo ASC CDL rOffset"]')!;input.value='-0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(11));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{rOffset:-0.125}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors eleven Transform keys using automatic source coordinates and preserves curves with precise edit undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={source:{id:'source',width:18,height:10}};const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind:'transform',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(TransformEffect,{target:document.body});await tick();
 const controls=[['scaleHeight',100],['scaleWidth',100],['skew',0],['skewAxis',0],['rotation',0],['opacity',100],['shutterAngle',0],['anchorX',9],['anchorY',5],['positionX',9],['positionY',5]] as const
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Transform ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);expect(effect.parameters).toEqual({});
 const x=document.querySelector<HTMLInputElement>('input[aria-label="photo Transform anchorX"]')!;x.value='1.123456789012';x.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(12));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{anchor:[1.123456789012,null]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 document.querySelector<HTMLInputElement>('input[aria-label="photo Transform uniformScale"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(12));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{uniformScale:false}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors eight standalone Vignette keys preserving fractional color and curves with undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vignette',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(StandaloneVignette,{target:document.body});await tick();
 const controls=[["amount", -50], ["midpoint", 50], ["roundness", 0], ["feather", 50], ["colorRed", 0], ["colorGreen", 0], ["colorBlue", 0], ["colorAlpha", 1]];
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Vignette ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Vignette colorRed"]')!;input.value='0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{color:[0.125,0,0,1]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors all64 Lighting Effects keys and preserves precise auto-center axes and curves through undo',async()=>{
 const {lightingChannels}=await import('$lib/composition/lightingEffects');
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const content=project.compositions.root.content;content.tracks[0].clips[0].effects=[{kind:'lighting_effects',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(LightingEffects,{target:document.body});await tick();
 const source=content.sources?.[content.tracks[0].clips[0].sourceId]??{};
 for(const[i,[channel]]of lightingChannels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Lighting Effects ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];
 for(const[channel,,, ,base]of lightingChannels){const value=channel.endsWith('CenterX')?(source.width??0)/2:channel.endsWith('CenterY')?(source.height??0)/2:base;expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])}
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Lighting Effects l1CenterX"]')!;input.value='1.123456789012';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(65));await tick();const updated=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(updated.animation).toEqual(effect.animation);expect(updated.parameters.lights[0].center).toEqual([1.123456789012,null]);expect(updated.parameters.lights[0].kind).toBe(3);expect(updated.parameters.lights).toHaveLength(5);
 undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
},20000)

it('authors six Logo Cutout keys and preserves color curves and source choices through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'logo_cutout',parameters:{background:2,unmultiply:false,invert:true}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(LogoCutout,{target:document.body});await tick();
 const controls=[['threshold',5],['softness',10],['colorRed',1],['colorGreen',1],['colorBlue',1],['colorAlpha',1]] as const;
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Logo Cutout ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Logo Cutout colorRed"]')!;input.value='0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,color:[0.125,1,1,1]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors five RGB Split keys preserving auto centers and radial mode through base edit undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={source:{id:'source',width:18,height:10}};const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='source';clip.effects=[{kind:'rgb_split',parameters:{mode:1}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(RgbSplit,{target:document.body});await tick();
 const channels=['amount','angle','centerX','centerY','blend'];for(const[i,c]of channels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo RGB Split ${c} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[c,value]of Object.entries({amount:10,angle:0,centerX:9,centerY:5,blend:0})){expect(effect.animation[c].track.keyframes).toEqual([{tick:500000,value}])}expect(effect.parameters).toEqual({mode:1});
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo RGB Split centerX"]')!;input.value='1.123456789012';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{mode:1,center:[1.123456789012,null]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
})

it('authors eight Edge Glow keys and preserves color curves and source choices through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'edge_glow',parameters:{only:true}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(EdgeGlow,{target:document.body});await tick();
 const controls=[['threshold',20],['width',2],['radius',10],['intensity',150],['colorRed',0.3],['colorGreen',0.8],['colorBlue',1],['colorAlpha',1]] as const;
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Edge Glow ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Edge Glow colorRed"]')!;input.value='0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,color:[0.125,0.8,1,1]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})


it('authors ten Echo Glow keys and preserves color curves and source choices through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'echo_glow',parameters:{echoes:3.75}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(EchoGlow,{target:document.body});await tick();
 const controls=[['threshold',60],['echoes',3.75],['radius',12],['spread',1.8],['intensity',100],['decay',50],['colorRed',1],['colorGreen',0.85],['colorBlue',0.6],['colorAlpha',1]] as const;
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Echo Glow ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Echo Glow colorRed"]')!;input.value='0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(11));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,color:[0.125,0.85,0.6,1]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})


it('authors eight Wonder Glow keys and preserves color curves and source choices through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'wonder_glow',parameters:{mode:1,useColor:true}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(WonderGlow,{target:document.body});await tick();
 const controls=[['threshold',60],['saturation',100],['radius',25],['intensity',100],['colorRed',1],['colorGreen',0.8],['colorBlue',0.5],['colorAlpha',1]] as const;
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Wonder Glow ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Wonder Glow colorRed"]')!;input.value='0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,color:[0.125,0.8,0.5,1]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})



it('authors nine Glint keys and preserves color curves and source choices through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'glint',parameters:{rays:3}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Glint,{target:document.body});await tick();
 const controls=[['threshold',75],['rotation',45],['colorize',0],['length',60],['intensity',100],['colorRed',1],['colorGreen',1],['colorBlue',1],['colorAlpha',1]] as const;
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Glint ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Glint colorRed"]')!;input.value='0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(10));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,color:[0.125,1,1,1]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors nine Volumetric Rays keys and preserves color curves and source choices through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'volumetric_rays',parameters:{only:true}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VolumetricRays,{target:document.body});await tick();
 const controls=[['threshold',60],['centerX',0],['centerY',0],['length',50],['intensity',100],['colorRed',1],['colorGreen',0.95],['colorBlue',0.8],['colorAlpha',1]] as const;
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Volumetric Rays ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Volumetric Rays colorRed"]')!;input.value='0.125';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(10));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,color:[0.125,0.95,0.8,1]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 const center=document.querySelector<HTMLInputElement>('input[aria-label="photo Volumetric Rays centerX"]')!;center.value='1.123456789012';center.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(10));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,center:[1.123456789012,null]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)

})

it('authors Posterize Time rate at local cursor preserving animation and exact undo',async()=>{
const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'posterize_time',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(PosterizeTime,{target:document.body});await tick();document.querySelector<HTMLButtonElement>('button[aria-label="photo Posterize Time rate key"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.rate.track.keyframes).toEqual([{tick:500000,value:12}]);const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Posterize Time rate"]')!;input.value='23.976';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{rate:23.976}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})
it('authors four Echo keys and changes operator preserving curves with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'echo',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Echo,{target:document.body});await tick();
 const controls=[['time',-0.033],['count',1],['start',1],['decay',1]] as const;
 for(const[channel]of controls){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Echo ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(controls.findIndex(([k])=>k===channel)+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Echo count"]')!;input.value='6.5';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{count:6.5}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo Echo operator"]')!;select.value='6';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{operator:6}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo Echo operator"]')!.value).toBe('0');
})
it('authors Auto Reframe keys preserving precise offset axes and restores both choices on undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'auto_reframe',parameters:{offset:[1.123456789012,-0.75]}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(AutoReframe,{target:document.body});await tick();
 const controls=[['zoom',100],['offsetX',1.123456789012],['offsetY',-0.75]]as const;
 for(const[channel]of controls){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Auto Reframe ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(controls.findIndex(([k])=>k===channel)+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Auto Reframe offsetX"]')!;input.value='2.123456789012';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{offset:[2.123456789012,-0.75]}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 for(const[channel,value,restored]of [['preset','2','1'],['aspect','4','0']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo Auto Reframe ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{...effect.parameters,[channel]:Number(value)}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo Auto Reframe ${channel}"]`)!.value).toBe(restored);}
})

it('authors all Warp Stabilizer channels and restores modes and flags with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'warp_stabilizer',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(WarpStabilizer,{target:document.body});await tick();
 const controls=[['smoothness',50],['maxScale',150],['actionSafe',0],['additionalScale',100],['cropLess',50]] as const;
 for(const[channel]of controls){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Warp Stabilizer ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(controls.findIndex(([key])=>key===channel)+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];
 for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo Warp Stabilizer smoothness"]')!;input.value='75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{smoothness:75}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 for(const[channel,value,restored]of [['result','1','0'],['method','3','1'],['framing','3','2']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo Warp Stabilizer ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{[channel]:Number(value)}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo Warp Stabilizer ${channel}"]`)!.value).toBe(restored);}
 for(const channel of ['preserveScale','detailed']as const){const checkbox=document.querySelector<HTMLInputElement>(`input[aria-label="photo Warp Stabilizer ${channel}"]`)!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{[channel]:true}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLInputElement>(`input[aria-label="photo Warp Stabilizer ${channel}"]`)!.checked).toBe(false);}
})


it('authors thirteen Light Leaks channels and preserves both colors and curves through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'light_leaks',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(LightLeaks,{target:document.body});await tick();
 const controls=[['intensity',70],['scale',100],['direction',30],['speed',1],['seed',0],['c1Red',1],['c1Green',0.45],['c1Blue',0.1],['c1Alpha',1],['c2Red',1],['c2Green',0.15],['c2Blue',0.35],['c2Alpha',1]]as const;
 for(const[i,[channel]]of controls.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Light Leaks ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of controls)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,color,value,expected]of [['c1Green','c1','0.125',[1,0.125,0.1,1]],['c2Alpha','c2','0.25',[1,0.15,0.35,0.25]]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Light Leaks ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(14));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{[color]:expected}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo Light Leaks mode"]')!;select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(14));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{mode:2}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo Light Leaks mode"]')!.value).toBe('0');
})

for(const[Inspector,kind,label,projection]of [[VrProjection,'vr_projection','VR Projection',true],[VrRotateSphere,'vr_rotate_sphere','VR Rotate Sphere',false]]as const)it(`authors ${label} angles and restores stereo layout and flags through undo`,async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind,parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Inspector,{target:document.body});await tick();
 const channels=projection?['tilt','pan','roll','fov']:['tilt','pan','roll'];
 for(const[i,channel]of channels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo ${label} ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const channel of channels)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:channel==='fov'?360:0}]);
 const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo ${label} pan"]`)!;input.value='-37.25';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(channels.length+1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{pan:-37.25}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo ${label} frameLayout"]`)!;select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(channels.length+1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{frameLayout:2}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo ${label} frameLayout"]`)!.value).toBe('0');
 if(projection){const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Projection stretch"]')!;expect(checkbox.checked).toBe(true);checkbox.checked=false;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(channels.length+1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{stretch:false}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLInputElement>('input[aria-label="photo VR Projection stretch"]')!.checked).toBe(true)}else expect(document.querySelector('input[aria-label="photo VR Rotate Sphere fov"]')).toBeNull();
})

for(const[Inspector,kind,label,channel]of [[VrBlur,'vr_blur','VR Blur','blurriness'],[VrSharpen,'vr_sharpen','VR Sharpen','amount']]as const)it(`authors ${label} scalar curve and restores stereo choice through undo`,async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind,parameters:{frameLayout:1}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Inspector,{target:document.body});await tick();document.querySelector<HTMLButtonElement>(`button[aria-label="photo ${label} ${channel} ключ"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:0}]);
 const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo ${label} ${channel}"]`)!;input.value='50.5';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{frameLayout:1,[channel]:50.5}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo ${label} frameLayout"]`)!;select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{frameLayout:2}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo ${label} frameLayout"]`)!.value).toBe('1');
})

it('authors VR De-Noise curve and restores stereo noise modes and show flag through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vr_denoise',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VrDenoise,{target:document.body});await tick();document.querySelector<HTMLButtonElement>('button[aria-label="photo VR De-Noise amount ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:0}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo VR De-Noise amount"]')!;input.value='50.5';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{amount:50.5}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 for(const[channel,value]of [['frameLayout','2'],['noiseType','1']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR De-Noise ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{[channel]:Number(value)}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR De-Noise ${channel}"]`)!.value).toBe('0');}
 const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo VR De-Noise showNoise"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{showNoise:true}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLInputElement>('input[aria-label="photo VR De-Noise showNoise"]')!.checked).toBe(false);
})

it('authors six VR Chromatic channels preserving stereo and inverse through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vr_chromatic_aberrations',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VrChromatic,{target:document.body});await tick();
 const channels=['red','green','blue','centerX','centerY','falloff'];for(const[i,channel]of channels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo VR Chromatic Aberrations ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const channel of channels)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:channel==='falloff'?50:0}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Chromatic Aberrations centerY"]')!;input.value='-47.75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{centerY:-47.75}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo VR Chromatic Aberrations frameLayout"]')!;select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{frameLayout:2}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo VR Chromatic Aberrations frameLayout"]')!.value).toBe('0');
 const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Chromatic Aberrations inverse"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{inverse:true}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLInputElement>('input[aria-label="photo VR Chromatic Aberrations inverse"]')!.checked).toBe(false);
})

it('authors twenty VR gradient keys and preserves color components and choices through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vr_color_gradients',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VrColorGradients,{target:document.body});await tick();
 const defaults={c1R:1,c1G:.25,c1B:.25,c1A:1,c2R:.25,c2G:1,c2B:.4,c2A:1,c3R:.3,c3G:.4,c3B:1,c3A:1,p1Lon:-120,p1Lat:20,p2Lon:0,p2Lat:-20,p3Lon:120,p3Lat:20,blend:50,opacity:100};
 for(const[i,channel]of Object.keys(defaults).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo VR Color Gradients ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(defaults))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['c2G','0.125',{c2:[.25,.125,.4,1]}],['p3Lat','-47.75',{p3Lat:-47.75}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo VR Color Gradients ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(21));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 for(const[channel,value]of [['frameLayout','2'],['mode','4']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR Color Gradients ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(21));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{[channel]:Number(value)}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR Color Gradients ${channel}"]`)!.value).toBe('0');}
})

it('authors six VR glitch channels preserving stereo through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vr_digital_glitch',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VrDigitalGlitch,{target:document.body});await tick();
 const channels=['amplitude','distortion','rate','color','scanlines','seed'];for(const[i,channel]of channels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo VR Digital Glitch ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const channel of channels)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:({amplitude:0,distortion:20,rate:10,color:50,scanlines:0,seed:0} as Record<string,number>)[channel]}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Digital Glitch seed"]')!;input.value='17.75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{seed:17.75}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo VR Digital Glitch frameLayout"]')!;select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{frameLayout:2}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo VR Digital Glitch frameLayout"]')!.value).toBe('0');
})

it('authors six VR fractal channels preserving choices and inversion through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vr_fractal_noise',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VrFractalNoise,{target:document.body});await tick();
 const channels=['contrast','brightness','scale','complexity','evolution','opacity'];for(const[i,channel]of channels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo VR Fractal Noise ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const channel of channels)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:({contrast:100,brightness:0,scale:100,complexity:6,evolution:0,opacity:100} as Record<string,number>)[channel]}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Fractal Noise evolution"]')!;input.value='123.75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{evolution:123.75}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 for(const[channel,value]of [['frameLayout','2'],['fractalType','2'],['mode','4']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR Fractal Noise ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{[channel]:Number(value)}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR Fractal Noise ${channel}"]`)!.value).toBe('0');}
 const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Fractal Noise invert"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{invert:true}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLInputElement>('input[aria-label="photo VR Fractal Noise invert"]')!.checked).toBe(false);
})

it('authors eight VR glow keys and preserves color components and choices through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vr_glow',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VrGlow,{target:document.body});await tick();
 const defaults={threshold:70,radius:30,brightness:1,saturation:1,tintR:1,tintG:1,tintB:1,tintA:1};
 for(const[i,channel]of Object.keys(defaults).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo VR Glow ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(defaults))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['tintG','0.125',{tint:[1,.125,1,1]}],['radius','12.25',{radius:12.25}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo VR Glow ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 for(const[channel,value]of [['frameLayout','2']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR Glow ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{[channel]:Number(value)}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>(`select[aria-label="photo VR Glow ${channel}"]`)!.value).toBe('0');}
 const checkbox=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Glow useTint"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{useTint:true}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLInputElement>('input[aria-label="photo VR Glow useTint"]')!.checked).toBe(false);
})

it('authors five VR plane channels preserving stereo through exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'vr_plane_to_sphere',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(VrPlaneToSphere,{target:document.body});await tick();
 const channels=['scale','pan','tilt','roll','feather'];for(const[i,channel]of channels.entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo VR Plane to Sphere ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const channel of channels)expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value:({scale:60,pan:0,tilt:0,roll:0,feather:0} as Record<string,number>)[channel]}]);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo VR Plane to Sphere roll"]')!;input.value='123.75';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{roll:123.75}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="photo VR Plane to Sphere frameLayout"]')!;select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{frameLayout:2}});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(document.querySelector<HTMLSelectElement>('select[aria-label="photo VR Plane to Sphere frameLayout"]')!.value).toBe('0');
})

it('authors Simple Text auto-position keys and preserves component edits through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={textSource:{id:'textSource',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0].sourceId='textSource';project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'simple_text',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(SimpleText,{target:document.body});await tick();
 const values:Record<string,number>={positionX:48,positionY:24,size:64,colorR:1,colorG:1,colorB:1,colorA:1,bgOpacity:0};
 for(const[i,channel]of Object.keys(values).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Simple Text ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(values))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['positionX','12.25',{position:[12.25,null]}],['colorG','0.125',{color:[1,.125,1,1]}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Simple Text ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 const text=document.querySelector<HTMLTextAreaElement>('textarea[aria-label="photo Simple Text text"]')!;text.value='Привет\nעולם';text.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{text:'Привет\nעולם'}});undoNestedComposition();await tick();expect(text.value).toBe('Simple Text');
 for(const[channel,value,defaultValue]of [['font','1','0'],['alignment','2','1']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo Simple Text ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(select.value).toBe(defaultValue);}
})

it('authors Metadata Burn-in auto-position keys and preserves component edits through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={textSource:{id:'textSource',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0].sourceId='textSource';project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'metadata_burnin',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(MetadataBurnin,{target:document.body});await tick();
 const values:Record<string,number>={positionX:48,positionY:24,size:6,colorR:1,colorG:1,colorB:1,colorA:1,opacity:60};
 for(const[i,channel]of Object.keys(values).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Metadata Burn-in ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(values))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['positionX','12.25',{position:[12.25,null]}],['colorG','0.125',{color:[1,.125,1,1]}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Metadata Burn-in ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 const text=document.querySelector<HTMLInputElement>('input[aria-label="photo Metadata Burn-in prefix"]')!;text.value='FILE Привет';text.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters:{prefix:'FILE Привет'}});undoNestedComposition();await tick();expect(text.value).toBe('');
 for(const[channel,value,defaultValue]of [['source','5','0'],['alignment','6','0']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo Metadata Burn-in ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(select.value).toBe(defaultValue);}
})

it('authors burn-in names and timecode settings with exact document undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.sources={file:{id:'file',width:96,height:48}};const clip=project.compositions.root.content.tracks[0].clips[0];clip.sourceId='file';clip.effects=[{kind:'metadata_burnin',parameters:{}}];nestedComposition.document=project;component=mount(MetadataBurnin,{target:document.body});await tick();
 const before=JSON.parse(JSON.stringify(nestedComposition.document));
 for(const[label,value,metadata]of [['Metadata Burn-in sequenceName','Seq Привет',{sequenceName:'Seq Привет'}],['Metadata Burn-in sequenceStartFrame','86400',{sequenceStartFrame:86400}],['photo Metadata Burn-in clipName','Clip',{clipNames:{photo:'Clip'}}],['photo Metadata Burn-in fileName','A001.mov',{sourceNames:{file:'A001.mov'}}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.metadata).toEqual(metadata);undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 const drop=document.querySelector<HTMLInputElement>('input[aria-label="Metadata Burn-in dropFrame"]')!;drop.checked=true;drop.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.metadata).toEqual({dropFrame:true});undoNestedComposition();await tick();expect(drop.checked).toBe(false);expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
})

it('authors Timecode source-centred keys and preserves edits through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={textSource:{id:'textSource',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0].sourceId='textSource';project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'timecode',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Timecode,{target:document.body});await tick();
 const values:Record<string,number>={positionX:48,positionY:24,size:15,opacity:100};
 for(const[i,channel]of Object.keys(values).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Timecode ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(values))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['positionX','12.25',{position:[12.25,null]}],['size','37.5',{size:37.5}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Timecode ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(Object.keys(values).length+1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
})

it('authors Clip Name source-centred keys and preserves edits through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={textSource:{id:'textSource',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0].sourceId='textSource';project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'clip_name',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(ClipName,{target:document.body});await tick();
 const values:Record<string,number>={positionX:48,positionY:24,size:15};
 for(const[i,channel]of Object.keys(values).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Clip Name ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(values))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['positionX','12.25',{position:[12.25,null]}],['size','37.5',{size:37.5}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Clip Name ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(Object.keys(values).length+1));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
})

it('authors Write-on auto-position keys and preserves component edits through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={textSource:{id:'textSource',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0].sourceId='textSource';project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'write_on',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(WriteOn,{target:document.body});await tick();
 const values:Record<string,number>={brushX:48,brushY:24,size:8,colorR:1,colorG:1,colorB:1,colorA:1,hardness:75,opacity:100,strokeLength:0,spacing:0.01};
 for(const[i,channel]of Object.keys(values).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Write-on ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(values))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['brushX','12.25',{brush:[12.25,null]}],['colorG','0.125',{color:[1,.125,1,1]}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Write-on ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(12));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 for(const[channel,value,defaultValue]of [['style','2','0']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo Write-on ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(12));await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(select.value).toBe(defaultValue);}
})


it('authors Lightning auto-position keys and preserves component edits through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={textSource:{id:'textSource',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0].sourceId='textSource';project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'lightning',parameters:{}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(Lightning,{target:document.body});await tick();
 const values:Record<string,number>={startX:96*.1,startY:48*.1,endX:96*.9,endY:48*.9,segments:7,amplitude:10,detail:2,detailAmplitude:.3,branching:.3,speed:10,width:10,core:.4,seed:0,outsideR:.35,outsideG:.3,outsideB:1,outsideA:1,insideR:1,insideG:1,insideB:1,insideA:1};
 for(const[i,channel]of Object.keys(values).entries()){document.querySelector<HTMLButtonElement>(`button[aria-label="photo Lightning ${channel} key"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(i+1));await tick();}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];for(const[channel,value]of Object.entries(values))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}]);
 for(const[channel,value,parameters]of [['startX','12.25',{start:[12.25,null]}],['outsideG','0.125',{outside:[.35,.125,1,1]}]]as const){const input=document.querySelector<HTMLInputElement>(`input[aria-label="photo Lightning ${channel}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(22));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({...effect,parameters});undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);}
 for(const[channel,value,defaultValue]of [['mode','4','1']]as const){const select=document.querySelector<HTMLSelectElement>(`select[aria-label="photo Lightning ${channel}"]`)!;select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(22));await tick();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);expect(select.value).toBe(defaultValue);}
})


it('preserves Track Matte choices through independent edits and exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'track_matte',parameters:{matte:3,composite:1,reverse:true}}]
 nestedComposition.document=project;component=mount(TrackMatte,{target:document.body});await tick()
 const before=JSON.parse(JSON.stringify(nestedComposition.document));
 const matte=document.querySelector<HTMLSelectElement>('select[aria-label="photo Track Matte matte"]')!;expect(matte.value).toBe('3');
 const composite=document.querySelector<HTMLSelectElement>('select[aria-label="photo Track Matte composite"]')!;expect(composite.value).toBe('1');
 matte.value='4';matte.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 composite.value='0';composite.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLInputElement>('input[aria-label="photo Track Matte reverse"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters).toEqual({matte:4,composite:0,reverse:false})
 for(let i=0;i<3;i++){undoNestedComposition();await tick()}
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
 expect(matte.value).toBe('3');expect(composite.value).toBe('1')
})

it('authors all GraphicShape keys with precise auto position and preserves flags/path through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={picture:{id:'picture',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0]={...project.compositions.root.content.tracks[0].clips[0],sourceId:'picture',effects:[{kind:'graphic_shape',parameters:{}}]};nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(GraphicShape,{target:document.body});await tick()
 const buttons=[...document.querySelectorAll<HTMLButtonElement>('button[aria-label^="photo GraphicShape "][aria-label$=" key"]')];expect(buttons).toHaveLength(42)
 for(const [index,button]of buttons.entries()){button.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(index+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.positionX.track.keyframes).toEqual([{tick:500000,value:48}]);expect(effect.animation.positionY.track.keyframes).toEqual([{tick:500000,value:24}]);expect(effect.animation.sizeX.track.keyframes).toEqual([{tick:500000,value:400}])
 const position=document.querySelector<HTMLInputElement>('input[aria-label="photo GraphicShape positionX"]')!;position.value='1.23456789012345';position.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(43));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.position).toEqual([1.23456789012345,24])
 document.querySelector<HTMLInputElement>('input[aria-label="photo GraphicShape uniformScale"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(44));await tick()
 const points=document.querySelector<HTMLTextAreaElement>('textarea[aria-label="photo GraphicShape points"]')!;points.value='[[0,0],[12,0],[0,12]]';points.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(45));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters.points).toEqual([[0,0],[12,0],[0,12]]);expect(changed.parameters.uniformScale).toBe(false);expect(changed.animation).toEqual(effect.animation)
 for(let i=0;i<3;i++){undoNestedComposition();await tick()}expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors all GraphicText keys with precise auto position and preserves flags/text through undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.canvas={width:128,height:64};project.compositions.root.content.sources={picture:{id:'picture',width:96,height:48}};project.compositions.root.content.tracks[0].clips[0]={...project.compositions.root.content.tracks[0].clips[0],sourceId:'picture',effects:[{kind:'graphic_text',parameters:{}}]};nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(GraphicText,{target:document.body});await tick()
 const buttons=[...document.querySelectorAll<HTMLButtonElement>('button[aria-label^="photo GraphicText "][aria-label$=" key"]:not([aria-label$="source key"])')];expect(buttons).toHaveLength(44)
 for(const [index,button]of buttons.entries()){button.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(index+1));await tick()}
 const before=JSON.parse(JSON.stringify(nestedComposition.document));const effect=before.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.positionX.track.keyframes).toEqual([{tick:500000,value:48}]);expect(effect.animation.positionY.track.keyframes).toEqual([{tick:500000,value:24}]);expect(effect.animation.size.track.keyframes).toEqual([{tick:500000,value:100}])
 const position=document.querySelector<HTMLInputElement>('input[aria-label="photo GraphicText positionX"]')!;position.value='1.23456789012345';position.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(45));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.position).toEqual([1.23456789012345,24])
 document.querySelector<HTMLInputElement>('input[aria-label="photo GraphicText uniformScale"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(46));await tick()
 const text=document.querySelector<HTMLInputElement>('input[aria-label="photo GraphicText text"]')!;text.value='Привет مرحبا';text.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(47));await tick()
 const changed=JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed.parameters.text).toBe('Привет مرحبا');expect(changed.parameters.uniformScale).toBe(false);expect(changed.animation).toEqual(effect.animation)
 for(let i=0;i<3;i++){undoNestedComposition();await tick()}expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before)
})

it('authors and edits held source text keys with exact undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'graphic_text',parameters:{text:'Привет مرحبا',font:'Noto Serif',runs:[{start:0,end:2,style:{underline:true}}]}}];nestedComposition.document=project;nestedComposition.timeUs=600000;component=mount(GraphicText,{target:document.body});await tick();
 const before=JSON.parse(JSON.stringify(nestedComposition.document));
 document.querySelector<HTMLButtonElement>('button[aria-label="photo GraphicText source key"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();
 const keyed=JSON.parse(JSON.stringify(nestedComposition.document));const effect=keyed.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.sourceTextAnimation).toEqual({timeBase:1000000,keyframes:[{tick:500000,value:'Привет مرحبا'}]});expect(effect.parameters).toEqual(before.compositions.root.content.tracks[0].clips[0].effects[0].parameters);
 const input=document.querySelector<HTMLInputElement>('input[aria-label="photo GraphicText source 500000 value"]')!;input.value='Новый текст';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].sourceTextAnimation.keyframes[0].value).toBe('Новый текст');undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(keyed);
 document.querySelector<HTMLButtonElement>('button[aria-label="photo GraphicText source 500000 delete"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].sourceTextAnimation).toBeUndefined();undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(keyed);undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(before);
})

it('applies native character formatting and retains it across source text insertion with undo',async()=>{
 const original={kind:'graphic_text',parameters:{text:'AБ🙂Z',font:'Inter'}};const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[original];nestedComposition.document=project;component=mount(GraphicText,{target:document.body});await tick();
 const runs=[{start:0,end:4,style:{fauxBold:true}}];vi.mocked(editGraphicTextRuns).mockResolvedValueOnce(runs);
 const bold=document.querySelector<HTMLSelectElement>('select[aria-label="photo GraphicText characters fauxBold"]')!;bold.value='true';bold.dispatchEvent(new Event('change',{bubbles:true}));await tick();[...document.querySelectorAll('button')].find(b=>b.textContent==='Применить к символам')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(editGraphicTextRuns).toHaveBeenLastCalledWith('AБ🙂Z',[],{kind:'apply',start:0,end:4,style:{fauxBold:true}});
 const styled=JSON.parse(JSON.stringify(nestedComposition.document));expect(styled.compositions.root.content.tracks[0].clips[0].effects[0].parameters.runs).toEqual(runs);
 const adjusted=[{start:0,end:5,style:{fauxBold:true}}];vi.mocked(editGraphicTextRuns).mockResolvedValueOnce(adjusted);const text=document.querySelector<HTMLInputElement>('input[aria-label="photo GraphicText text"]')!;text.value='AБX🙂Z';text.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(editGraphicTextRuns).toHaveBeenLastCalledWith('AБ🙂Z',runs,{kind:'adjust',newText:'AБX🙂Z'});expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].parameters.runs).toEqual(adjusted);undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(styled);undoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(project);
})
