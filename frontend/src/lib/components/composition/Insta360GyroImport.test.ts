// @ts-expect-error Tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import {analyzeInsta360Gyro,inspectInsta360Source} from '$lib/api'
import Panel from './Insta360GyroImport.svelte'
vi.mock('$lib/api',()=>({analyzeInsta360Gyro:vi.fn(),inspectInsta360Source:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const response={status:'sensor_axes_required' as const,mountingSource:'unresolved' as const,synchronization:'first_frame_capture_clock',limitations:[],inputSamples:100,captureStartSeconds:0,captureEndSeconds:1,samples:[],orientation:[]}
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.clearAllMocks()})
it('keeps existing orientation when mounting is unknown and sends the trimmed source clock',async()=>{
 vi.mocked(analyzeInsta360Gyro).mockResolvedValue(response);const onapply=vi.fn()
 component=mount(Panel,{target:document.body,props:{sourceId:'source',token:'token',startSeconds:12,durationSeconds:1,binding:{},onapply}});await tick()
 document.querySelector<HTMLButtonElement>('button')!.click()
 await vi.waitFor(()=>expect(document.body.textContent).toContain('нужна калибровка осей'))
 expect(analyzeInsta360Gyro).toHaveBeenCalledWith('source','token',expect.objectContaining({startSeconds:12,durationSeconds:1,cameraFromSensor:undefined,orientationToleranceDegrees:.005,motionPaddingSeconds:0}),expect.any(AbortSignal));expect(onapply).not.toHaveBeenCalled()
})
it.each([['unavailable_dynamic_start','абсолютный горизонт не подтверждён'],['estimated_observed_preroll','точное выравнивание не подтверждено']])('applies valid relative orientation while displaying %s gravity readiness',async(status,warning)=>{
 const orientation=[{timeSeconds:0,quaternion:[1,0,0,0] as const}];vi.mocked(analyzeInsta360Gyro).mockResolvedValue({...response,status:'orientation_ready',orientation,gravityAlignment:{status,warning:'Gravity estimate is uncertain',requestedForceNormG:1.24,observedPrerollSeconds:status==='estimated_observed_preroll'?.73:0,initializationSamples:status==='estimated_observed_preroll'?52:0}});const onapply=vi.fn()
 component=mount(Panel,{target:document.body,props:{sourceId:'source',token:'token',startSeconds:12,durationSeconds:1,binding:{},onapply}});await tick()
 const field=document.querySelector<HTMLInputElement>('[aria-label="Gyro camera from sensor"]')!;field.value='[1,0,0,0]';field.dispatchEvent(new Event('input',{bubbles:true}));await tick();document.querySelector<HTMLButtonElement>('button')!.click()
 await vi.waitFor(()=>expect(onapply).toHaveBeenCalledWith(expect.objectContaining({orientation,orientationOffsetSeconds:0})))
 expect(document.body.textContent).toContain(warning)
})
it('aborts a pending native read on unmount without applying its result',async()=>{
 let finish!:(value:typeof response)=>void;vi.mocked(analyzeInsta360Gyro).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));const onapply=vi.fn()
 component=mount(Panel,{target:document.body,props:{sourceId:'source',token:'token',startSeconds:0,durationSeconds:1,binding:{},onapply}});await tick();document.querySelector<HTMLButtonElement>('button')!.click();await tick()
 const signal=vi.mocked(analyzeInsta360Gyro).mock.calls[0]![3]!;await unmount(component);component=undefined;expect(signal.aborted).toBe(true);finish(response);await tick();expect(onapply).not.toHaveBeenCalled()
})

it.each(['exposure_midpoint_linear','exposure_midpoint_trim_map'])('imports proven %s, precision guard bands and sensor readout together in one update',async(synchronization)=>{
 const orientation=[{timeSeconds:0,quaternion:[1,0,0,0] as const},{timeSeconds:1.2,quaternion:[1,0,0,0] as const}],captureClock=synchronization==='exposure_midpoint_trim_map'?[{timeSeconds:0,captureSeconds:20.9},{timeSeconds:1.2,captureSeconds:22.1}]:[{timeSeconds:0,captureSeconds:-.1},{timeSeconds:1.2,captureSeconds:1.1}],sourceFrameClock={numerator:50,denominator:1,sourceStartSeconds:0};
 vi.mocked(analyzeInsta360Gyro).mockResolvedValue({...response,status:'orientation_ready',synchronization,originalTimeMapOffsetSeconds:synchronization==='exposure_midpoint_trim_map'?21:undefined,orientation,orientationOffsetSeconds:.1,captureClock,sourceFrameClock});vi.mocked(inspectInsta360Source).mockResolvedValue({version:3,cameraModel:'Insta360 X6',firmware:null,frameRate:50,dimension:null,resolution:null,calibrations:[],blocks:[],rollingShutterTime:15.36});const onapply=vi.fn();
 component=mount(Panel,{target:document.body,props:{sourceId:'source',token:'token',startSeconds:0,durationSeconds:1,binding:{},allowRollingShutter:true,onapply}});await tick();const label=Array.from(document.querySelectorAll('label')).find(l=>l.textContent?.includes('Коррекция rolling shutter'))!;const checkbox=label.querySelector<HTMLInputElement>('input')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick();document.querySelector<HTMLButtonElement>('button')!.click();
 await vi.waitFor(()=>expect(onapply).toHaveBeenCalledWith(expect.objectContaining({orientation,orientationOffsetSeconds:.1,sourceFrameClock,rollingShutter:{readoutSeconds:expect.closeTo(.01536,12),referenceRow:.5,scanDirections:['top_to_bottom','top_to_bottom'],captureClock}})));
 expect(analyzeInsta360Gyro).toHaveBeenCalledWith('source','token',expect.objectContaining({orientationToleranceDegrees:.005,motionPaddingSeconds:.1}),expect.any(AbortSignal));
})
it('leaves effects unchanged if automatic sensor row timing cannot be established',async()=>{
 vi.mocked(analyzeInsta360Gyro).mockResolvedValue({...response,status:'orientation_ready',orientation:[{timeSeconds:0,quaternion:[1,0,0,0]}]});vi.mocked(inspectInsta360Source).mockResolvedValue(null);const onapply=vi.fn();component=mount(Panel,{target:document.body,props:{sourceId:'source',token:'token',startSeconds:0,durationSeconds:1,binding:{},allowRollingShutter:true,onapply}});await tick();const checkbox=Array.from(document.querySelectorAll('label')).find(l=>l.textContent?.includes('Коррекция rolling shutter'))!.querySelector<HTMLInputElement>('input')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick();document.querySelector<HTMLButtonElement>('button')!.click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Для rolling shutter нужны подтверждённые часы кадров'));expect(onapply).not.toHaveBeenCalled();
})
