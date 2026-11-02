// @ts-expect-error Tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import {inspectInsta360Profile,type Insta360ProfileAnalysis} from '$lib/api'
import Panel from './Insta360ProfileImport.svelte'
vi.mock('$lib/api',()=>({inspectInsta360Profile:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const response:Insta360ProfileAnalysis={calibration:{status:'unsupported_calibration',renderReady:false,reason:'Unknown calibration',calibrationField:111,profile:null},sourceLayout:null,packingReason:'Unknown lens track layout'}
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.clearAllMocks()})
it('retains the existing effect when calibration or source packing is unsupported',async()=>{
 vi.mocked(inspectInsta360Profile).mockResolvedValue(response);const onapply=vi.fn();component=mount(Panel,{target:document.body,props:{sourceId:'camera',token:'token',binding:{},onapply}});await tick();document.querySelector<HTMLButtonElement>('button')!.click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Unknown lens track layout'));expect(onapply).not.toHaveBeenCalled()
})
it('aborts pending calibration reads on unmount',async()=>{
 let finish!:(value:Insta360ProfileAnalysis)=>void;vi.mocked(inspectInsta360Profile).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));const onapply=vi.fn();component=mount(Panel,{target:document.body,props:{sourceId:'camera',token:'token',binding:{},onapply}});await tick();document.querySelector<HTMLButtonElement>('button')!.click();await tick();const signal=vi.mocked(inspectInsta360Profile).mock.calls[0]![2]!;await unmount(component);component=undefined;expect(signal.aborted).toBe(true);finish(response);await tick();expect(onapply).not.toHaveBeenCalled()
})
