// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {analyzeScopesBinary,renderNestedPreview,type NativeNestedProject,type NativeScopes} from '$lib/api'
import Scopes from './NestedScopes.svelte'
import ScopeCacheHarness from './fixtures/ScopeCacheHarness.svelte'
vi.mock('$lib/api',()=>({analyzeScopesBinary:vi.fn(),renderNestedPreview:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const project:NativeNestedProject={schemaVersion:1,root:'root',compositions:{}}
const put=vi.fn()
function result(kind='waveform',red=12):NativeScopes{return {samples:[1,1],matrix:'Bt709',unit:'%',stats:{r:{min:0,max:100,mean:50},g:{min:0,max:0,mean:0},b:{min:0,max:0,mean:0},y:{min:0,max:21.26,mean:10.63},chromaMean:[0,0],hueDeg:0,saturation:0},[kind]:{image:{width:1,height:1,rgba:[red,0,0,255]}}}}
beforeEach(()=>{
 try {window.localStorage.removeItem('video-kadr.scopes.v1')}catch{/* Storage may be unavailable in the test environment. */}
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({clearRect:vi.fn(),createImageData:(w:number,h:number)=>({data:new Uint8ClampedArray(w*h*4)}),putImageData:put} as unknown as CanvasRenderingContext2D)
 vi.mocked(renderNestedPreview).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray([255,0,0,128])})
 vi.mocked(analyzeScopesBinary).mockResolvedValue(result())
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;vi.restoreAllMocks();vi.clearAllMocks();document.body.innerHTML=''})
async function open(){const details=document.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()}
it('analyzes only an opened panel and paints native pixels with channel levels',async()=>{
 component=mount(Scopes,{target:document.body,props:{project,timeUs:600000,renderedOutputs:{root:'movie'},token:'session',compositionId:'root'}});await tick()
 expect(renderNestedPreview).not.toHaveBeenCalled();await open();await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(1))
 expect(vi.mocked(renderNestedPreview).mock.calls[0]?.slice(0,4)).toEqual([project,600000,{root:'movie'},'session'])
 expect(vi.mocked(analyzeScopesBinary).mock.calls[0]?.[0]).toEqual({width:1,height:1,rgba:new Uint8ClampedArray([255,0,0,128])})
 expect(put.mock.calls[0]?.[0].data).toEqual(new Uint8ClampedArray([12,0,0,255]))
 expect(document.querySelector('table')?.textContent).toContain('10.63')
 const signal=vi.mocked(analyzeScopesBinary).mock.calls[0]?.[2]!
 await unmount(component);component=undefined;expect(signal.aborted).toBe(true)
})
it('ignores superseded scope results after changing the selected graph',async()=>{
 let stale!:(value:NativeScopes)=>void
 vi.mocked(analyzeScopesBinary).mockImplementationOnce(()=>new Promise(resolve=>{stale=resolve})).mockResolvedValue(result('histogram',99))
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick();await open()
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(1))
 const oldSignal=vi.mocked(analyzeScopesBinary).mock.calls[0]?.[2]!
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes график"]')!;select.value='histogram';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(2))
 await vi.waitFor(()=>expect(put.mock.calls.length,document.querySelector('[role=alert]')?.textContent ?? JSON.stringify(vi.mocked(analyzeScopesBinary).mock.calls.map(c=>c[1]))).toBe(1));expect(oldSignal.aborted).toBe(true)
 expect(vi.mocked(analyzeScopesBinary).mock.calls[1]?.[1]?.scopes).toEqual(['histogram'])
 stale(result());await tick();expect(put).toHaveBeenCalledTimes(1);expect(put.mock.calls[0]?.[0].data[0]).toBe(99)
})
it('does not fetch an unauthenticated preview',async()=>{
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:''}});await tick();await open()
 expect(renderNestedPreview).not.toHaveBeenCalled();expect(analyzeScopesBinary).not.toHaveBeenCalled();expect(document.body.textContent).toContain('Войдите')
})

it('renders native vectorscope marks and forwards the target amplitude',async()=>{
 const response=result('vectorscopeYuv')
 response.vectorscopeYuv!.overlay=[{kind:'circle',cx:0.5,cy:0.5,r:0.4,color:'#404040'},{kind:'line',x1:0.5,y1:0.5,x2:0.2,y2:0.1,color:'#aaa'},{kind:'rect',x:0.1,y:0.1,width:0.1,height:0.1,color:'#f00'},{kind:'text',x:0.2,y:0.2,text:'R',color:'#f00'}]
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick()
 const kind=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes график"]')!;kind.value='vectorscopeYuv';kind.dispatchEvent(new Event('change',{bubbles:true}));await tick();await open()
 await vi.waitFor(()=>expect(document.querySelectorAll('svg circle')).toHaveLength(1))
 expect(document.querySelector('svg text')?.textContent).toBe('R');expect(document.querySelector('svg line')?.getAttribute('x2')).toBe('0.2')
 const targets=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes мишени"]')!;targets.value='100';targets.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(vi.mocked(analyzeScopesBinary).mock.calls.at(-1)?.[1]?.targets).toBe('100'))
})

it('shows native axis labels and requests float scale',async()=>{
 const response=result();response.waveform!.axis=[{position:50,left:'50',right:'128'}]
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick();await open()
 await vi.waitFor(()=>expect(document.querySelector('.axis.right')?.textContent).toBe('128'))
 expect(document.querySelector('.axis.left span')?.getAttribute('style')).toContain('50%')
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes шкала"]')!;select.value='float';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(vi.mocked(analyzeScopesBinary).mock.calls.at(-1)?.[1]?.scale).toBe('float'))
})

it('restores user scope controls after the workspace panel is remounted',async()=>{
 const data=new Map<string,string>()
 vi.spyOn(window,'localStorage','get').mockReturnValue({getItem:(key:string)=>data.get(key)??null,setItem:(key:string,value:string)=>{data.set(key,value)}} as Storage)
 const props={project,timeUs:0,renderedOutputs:{},token:'session'}
 component=mount(Scopes,{target:document.body,props});await tick()
 const scale=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes шкала"]')!;scale.value='float';scale.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 await unmount(component);component=undefined;document.body.innerHTML=''
 component=mount(Scopes,{target:document.body,props});await tick()
 expect(document.querySelector<HTMLSelectElement>('select[aria-label="Scopes шкала"]')?.value).toBe('float')
 expect(document.querySelector<HTMLDetailsElement>('details')?.open).toBe(false)
 expect(renderNestedPreview).not.toHaveBeenCalled()
})

it('analyzes multiple selected scopes from a single shared preview and keeps at least one scope',async()=>{
 const response=result();response.histogram={image:{width:1,height:1,rgba:[99,0,0,255]}}
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick()
 const histogram=document.querySelector<HTMLInputElement>('input[aria-label="Scopes показать Гистограмма"]')!;histogram.checked=true;histogram.dispatchEvent(new Event('change',{bubbles:true}));await tick();await open()
 await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(2))
 expect(renderNestedPreview).toHaveBeenCalledTimes(1);expect(analyzeScopesBinary).toHaveBeenCalledTimes(1)
 expect(vi.mocked(analyzeScopesBinary).mock.calls[0]?.[1]?.scopes).toEqual(['waveform','histogram'])
 expect(document.querySelectorAll('canvas')).toHaveLength(2)
 const wave=document.querySelector<HTMLInputElement>('input[aria-label="Scopes показать Waveform"]')!;wave.checked=false;wave.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(histogram.disabled).toBe(true)
})

it('applies the source four-scope YUV preset atomically and retains unrelated matrix and targets',async()=>{
 const response=result();for(const kind of ['vectorscopeYuv','histogram','parade'] as const)response[kind]={image:{width:1,height:1,rgba:[1,2,3,255]}}
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick()
 const matrix=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes матрица"]')!;matrix.value='Bt601';matrix.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const presets=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes пресет"]')!;expect(presets.options.length).toBe(9)
 presets.value='Four Scopes (YUV, float, no clamp)';presets.dispatchEvent(new Event('change',{bubbles:true}));await tick();await open()
 await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(4))
 expect(analyzeScopesBinary).toHaveBeenCalledTimes(1)
 expect(vi.mocked(analyzeScopesBinary).mock.calls[0]?.[1]).toMatchObject({scopes:['vectorscopeYuv','histogram','parade','waveform'],waveformType:'yc',paradeType:'yuv',scale:'float',clamp:false,matrix:'Bt601',targets:'75'})
 const scale=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes шкала"]')!;scale.value='bits8';scale.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(presets.value).toBe('')
})

it('uses the native layout response for graph columns and height',async()=>{
 const response=result();response.layout={columns:3,rows:2}
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick();await open()
 await vi.waitFor(()=>expect(document.querySelector('.graphs')?.getAttribute('style')).toContain('repeat(3,'))
 expect(vi.mocked(analyzeScopesBinary).mock.calls[0]?.[1]).toMatchObject({layoutWidth:600,layoutHeight:440})
})

it('reuses an authorized frame for scope settings changes without fetching media again',async()=>{
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick();await open()
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(1))
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes матрица"]')!;select.value='Bt601';select.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(2))
 expect(renderNestedPreview).toHaveBeenCalledTimes(1)
 expect(vi.mocked(analyzeScopesBinary).mock.calls[1]?.[1]?.matrix).toBe('Bt601')
 const details=document.querySelector('details')!;details.open=false;details.dispatchEvent(new Event('toggle'));await tick();await open()
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(3))
 expect(renderNestedPreview).toHaveBeenCalledTimes(1)
})

it('invalidates the cached preview when cursor or session changes',async()=>{
 component=mount(ScopeCacheHarness,{target:document.body,props:{project}});await tick();await open()
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(1))
 document.querySelectorAll('button')[0]!.click()
 await vi.waitFor(()=>expect(renderNestedPreview).toHaveBeenCalledTimes(2))
 expect(vi.mocked(renderNestedPreview).mock.calls[1]?.slice(1,4)).toEqual([1000000,{root:'movie'},'first-session'])
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(2))
 document.querySelectorAll('button')[1]!.click()
 await vi.waitFor(()=>expect(renderNestedPreview).toHaveBeenCalledTimes(3))
 expect(vi.mocked(renderNestedPreview).mock.calls[2]?.[3]).toBe('second-session')
})

it('paints native parade separators above the density image',async()=>{
 const response=result('parade');response.parade!.foreground=[{kind:'line',x1:0.33,y1:0,x2:0.33,y2:1,color:'#282828'}]
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick()
 const kind=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes график"]')!;kind.value='parade';kind.dispatchEvent(new Event('change',{bubbles:true}));await tick();await open()
 await vi.waitFor(()=>expect(document.querySelector('svg.foreground line')?.getAttribute('x1')).toBe('0.33'))
 expect(document.querySelector('.graph')?.lastElementChild?.classList.contains('foreground')).toBe(true)
})

it('requests the source HDR axis and displays native nits labels',async()=>{
 const response=result();response.waveform!.axisUnit='cd/m²';response.waveform!.axis=[{position:40,left:'203',right:''}]
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick()
 const scale=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes шкала"]')!;scale.value='hdr';scale.dispatchEvent(new Event('change',{bubbles:true}));await tick();await open()
 await vi.waitFor(()=>expect(document.querySelector('.axis-unit')?.textContent).toBe('cd/m²'))
 expect(vi.mocked(analyzeScopesBinary).mock.calls[0]?.[1]?.scale).toBe('hdr')
 expect(document.querySelector('.axis.left')?.textContent).toBe('203')
})

it('forwards automatic color-space resolution to native analysis',async()=>{
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick()
 const matrix=document.querySelector<HTMLSelectElement>('select[aria-label="Scopes матрица"]')!;matrix.value='auto';matrix.dispatchEvent(new Event('change',{bubbles:true}));await tick();await open()
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(1))
 expect(vi.mocked(analyzeScopesBinary).mock.calls[0]?.[1]?.colorSpace).toBe('auto')
 expect(vi.mocked(analyzeScopesBinary).mock.calls[0]?.[1]?.matrix).toBeUndefined()
})

it('retains the last compatible graph while a cursor frame is pending and clears it on session change',async()=>{
 let finish!:(value:NativeScopes)=>void
 vi.mocked(analyzeScopesBinary).mockResolvedValueOnce(result()).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve})).mockImplementation(()=>new Promise(()=>{}))
 component=mount(ScopeCacheHarness,{target:document.body,props:{project}});await tick();await open()
 await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(1))
 document.querySelectorAll('button')[0]!.click()
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(2))
 expect(document.querySelector('table')).not.toBeNull();expect(document.querySelector('.graphs')?.getAttribute('aria-busy')).toBe('true')
 expect(put).toHaveBeenCalledTimes(1)
 finish(result('waveform',99));await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(2));expect(put.mock.calls[1]?.[0].data[0]).toBe(99)
 document.querySelectorAll('button')[1]!.click();await tick()
 expect(document.querySelector('table')).toBeNull()
})

it('opens shared scope settings by context menu and restores graph focus on Escape',async()=>{
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick();await open()
 await vi.waitFor(()=>expect(analyzeScopesBinary).toHaveBeenCalledTimes(1))
 const graph=document.querySelector<HTMLElement>('.graphs')!;graph.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,clientX:100,clientY:100}));await tick()
 const menu=document.querySelector('[role="dialog"]')!;expect(menu).not.toBeNull()
 const preset=menu.querySelector<HTMLSelectElement>('select[aria-label="Scopes пресет"]')!;preset.value='Waveform (Luma)';preset.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(vi.mocked(analyzeScopesBinary).mock.calls.at(-1)?.[1]?.waveformType).toBe('luma'))
 window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await tick()
 expect(document.querySelector('[role="dialog"]')).toBeNull();expect(document.activeElement).toBe(graph)
 graph.dispatchEvent(new KeyboardEvent('keydown',{key:'F10',shiftKey:true,bubbles:true}));await tick();expect(document.querySelector('[role="dialog"]')).not.toBeNull()
 document.body.dispatchEvent(new Event('pointerdown',{bubbles:true}));await tick();expect(document.querySelector('[role="dialog"]')).toBeNull()
})

it('shows the native mode caption and resolved automatic color space',async()=>{
 const response=result();response.waveform!.title='Waveform (Luma)';response.resolvedColorSpace='rec709'
 vi.mocked(analyzeScopesBinary).mockResolvedValue(response)
 component=mount(Scopes,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'session'}});await tick();await open()
 await vi.waitFor(()=>expect(document.querySelector('.scope-name')?.textContent).toBe('Waveform (Luma)'))
 expect(document.querySelector('[aria-label="Цветовое пространство Scopes"]')?.textContent).toBe('Rec. 709')
})
