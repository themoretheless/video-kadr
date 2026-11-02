// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Import from './CaptionFileImport.svelte'
import {prepareCaptionFileImport,type PreparedCaptionFileImport} from '$lib/api'
import {createComposition} from '$lib/composition/commands'
vi.mock('$lib/api',()=>({prepareCaptionFileImport:vi.fn()}))
const prepared:PreparedCaptionFileImport={name:'talk',format:'Subtitle',language:'en',enabled:true,locked:false,syncLock:true,backgroundColor:[0,0,0,191],document:{ticksPerSecond:'254016000000',blocks:['NOTE retained'],warnings:['Source warning'],cues:[{startTick:'137729592000',endTick:'508540032000',text:'Hello',speaker:'Ann',id:'cue-1',settings:'align:start'}]}}
function document(){return createComposition({width:320,height:180,fps:24000/1001,fpsRate:{numerator:24000,denominator:1001},backgroundColor:'#000000'})}
function choose(target:HTMLElement){const input=target.querySelector('input')!;Object.defineProperty(input,'files',{value:[new File(['WEBVTT'],'talk.vtt')],configurable:true});input.dispatchEvent(new Event('change',{bubbles:true}));return input}
it('prepares file with exact rate, commits and activates once, preserving fractional ticks and warnings',async()=>{
 vi.mocked(prepareCaptionFileImport).mockReset().mockResolvedValue(prepared)
 const target=globalThis.document.createElement('div'),composition=document(),onchange=vi.fn(),onactive=vi.fn()
 const component=mount(Import,{target,props:{composition,onchange,onactive}});await tick();choose(target)
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(prepareCaptionFileImport).toHaveBeenCalledWith(btoa('WEBVTT'),'talk.vtt',{num:24000,den:1001})
 const track=onchange.mock.calls[0][0].captionTracks[0];expect(track.clips[0]).toMatchObject({timelineStartTick:'137729592000',timelineEndTick:'508540032000',cueId:'cue-1',speaker:'Ann',settings:'align:start'})
 expect(onactive).toHaveBeenCalledWith(track.id);expect(target.textContent).toContain('Source warning');expect(composition.captionTracks).toBeUndefined()
 await unmount(component)
})
it('refuses stale import if authored composition changed while preparing file',async()=>{
 let resolve!:(value:PreparedCaptionFileImport)=>void
 vi.mocked(prepareCaptionFileImport).mockReset().mockReturnValue(new Promise(done=>{resolve=done}))
 const target=globalThis.document.createElement('div'),composition=document(),onchange=vi.fn(),onactive=vi.fn()
 const component=mount(Import,{target,props:{composition,onchange,onactive}});await tick();choose(target)
 await vi.waitFor(()=>expect(prepareCaptionFileImport).toHaveBeenCalledOnce());Object.assign(composition.canvas,{width:640});resolve(prepared)
 await vi.waitFor(()=>expect(target.querySelector('[role=alert]')?.textContent).toContain('Композиция изменилась'))
 expect(onchange).not.toHaveBeenCalled();expect(onactive).not.toHaveBeenCalled();await unmount(component)
})
