// @ts-expect-error Tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import {analyseNestedImageCutout,nestedComposition} from '$lib/state/nestedComposition.svelte'
import Panel from './NestedConcatCutout.svelte'
vi.mock('$lib/state/nestedComposition.svelte',()=>({nestedComposition:{projectId:'saved',revision:7},nestedHistory:{busy:false},analyseNestedImageCutout:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.resetAllMocks()})
it('submits selected subject and feather and cancels its pending analysis',async()=>{
 let finish!:(result:boolean)=>void;vi.mocked(analyseNestedImageCutout).mockReturnValue(new Promise(resolve=>{finish=resolve}))
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'image'}});await tick()
 const select=document.querySelector('select')!;select.value='person';select.dispatchEvent(new Event('change',{bubbles:true}))
 const feather=document.querySelector('input')!;feather.value='0.02';feather.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 document.querySelector('button')!.click();await tick()
 expect(analyseNestedImageCutout).toHaveBeenCalledWith('owner','timeline','image','person',0.02,expect.any(AbortSignal))
 const signal=vi.mocked(analyseNestedImageCutout).mock.calls[0]![5]!
 Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Отменить анализ')!.click();expect(signal.aborted).toBe(true);finish(false);await tick()
})

it.skipIf(!process.env.CONCAT_CUTOUT_UI_REQUEST_OUTPUT)('captures mounted controls through the real API serializer for native HTTP replay',async()=>{
 const {analyseNestedCutout}=await import('$lib/api'),{writeFile}=await import('node:fs/promises')
 let written=false;nestedComposition.revision=1
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,options?:RequestInit)=>{
  await writeFile(process.env.CONCAT_CUTOUT_UI_REQUEST_OUTPUT!,String(options?.body));written=true
  return new Response('{}',{headers:{'content-type':'application/json'}})
 }))
 vi.mocked(analyseNestedImageCutout).mockImplementation(async(token,compositionId,clipId,subject,feather,signal)=>{await analyseNestedCutout(nestedComposition.projectId!,compositionId,clipId,nestedComposition.revision!,subject,feather,token,signal);return false})
 try{
  component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'clip'}});await tick()
  const select=document.querySelector('select')!;select.value='person';select.dispatchEvent(new Event('change',{bubbles:true}))
  const feather=document.querySelector('input')!;feather.value='0.02';feather.dispatchEvent(new Event('input',{bubbles:true}));await tick();document.querySelector('button')!.click()
  await vi.waitFor(()=>expect(written).toBe(true))
 }finally{vi.unstubAllGlobals()}
})
