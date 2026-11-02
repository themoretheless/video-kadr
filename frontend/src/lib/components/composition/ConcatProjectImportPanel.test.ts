import {writeFileSync} from 'node:fs'
// @ts-expect-error Svelte client entry is required for mounted components under Vitest.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {importConcatProject,evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Panel from './ConcatProjectImportPanel.svelte'
vi.mock('$lib/api',()=>({importConcatProject:vi.fn(),evaluateNestedFrame:vi.fn()}))
const original={version:1,name:'Original cut',media:[{id:'m1',name:'Camera.mov',kind:'video',path:'/never/open-this.mov'}],activeTimelineId:'TL1',timelines:[{id:'TL1',name:'Main',video:{width:16,height:16,rateNum:4,rateDen:1},tracks:[{id:'T1'}],clips:[{id:'clip',kind:'video',trackId:'T1',mediaId:'m1',start:1,duration:1,videoEffects:[{id:'concat.exposure',keys:{stops:[{at:0,value:0},{at:1,value:4,ease:[0.42,-0.5,0.58,1]}]}}]}]}]}
const source={id:'source',kind:'video' as const,durationTicks:1000000,width:16,height:16,hasAudio:false}
const project:NativeNestedProject={schemaVersion:1,root:'imported',compositions:{imported:{content:{schemaVersion:1,timeBase:1000000,canvas:{width:16,height:16,fpsMilli:4000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}},sources:{},tracks:[]},instances:[]}}}
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.clearAllMocks();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});vi.mocked(importConcatProject).mockResolvedValue({schemaVersion:1,status:'converted',project,diagnostics:[],sourceDocument:original,timelineIds:{TL1:'imported'}})})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();document.body.innerHTML=''})
function click(text:string){const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===text)!;expect(button).toBeDefined();button.click()}
async function read(){const input=document.querySelector<HTMLInputElement>('input[type=file]')!;const file=new File([JSON.stringify(original)],'concat.json',{type:'application/json'});Object.defineProperty(file,'text',{value:async()=>JSON.stringify(original)});Object.defineProperty(input,'files',{configurable:true,value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(document.querySelector('select[aria-label="Источник Concat m1"]')).not.toBeNull());await tick();const select=document.querySelector<HTMLSelectElement>('select[aria-label="Источник Concat m1"]')!;select.value='source';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()}
it('loads a file, binds registered media, reviews and adopts with undo/redo',async()=>{
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Library.mov',source}]}});await tick();await read();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenCalledWith(original,{m1:source},expect.any(AbortSignal),'session'));await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'));expect(nestedComposition.document).toBeNull()
 if(process.env.CONCAT_UI_IMPORT_REQUEST_OUTPUT){const[document,mediaBindings]=vi.mocked(importConcatProject).mock.calls[0]!;writeFileSync(process.env.CONCAT_UI_IMPORT_REQUEST_OUTPUT,JSON.stringify({schemaVersion:1,document,mediaBindings}))}
 click('Открыть импорт как новый проект');await vi.waitFor(()=>expect(nestedComposition.document).toEqual(project));expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.projectId).toBeNull();expect(nestedComposition.name).toBe('Original cut');undoNestedComposition();expect(nestedComposition.document).toBeNull();redoNestedComposition();expect(nestedComposition.document).toEqual(project)
})
it('shows import diagnostics without replacing the current editor',async()=>{
 vi.mocked(importConcatProject).mockResolvedValue({schemaVersion:1,status:'needs_attention',project:null,diagnostics:['crop: not converted'],sourceDocument:original,timelineIds:{}})
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Library.mov',source}]}});await tick();await read();click('Проверить импорт Concat');await vi.waitFor(()=>expect(document.body.textContent).toContain('crop: not converted'));expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect([...document.querySelectorAll('button')].some(b=>b.textContent?.includes('Открыть импорт'))).toBe(false)
})

it('ignores a conversion response after the editor target changes',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof importConcatProject>>)=>void;vi.mocked(importConcatProject).mockReturnValue(new Promise(done=>{resolve=done}))
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Library.mov',source}]}});await tick();await read();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenCalled())
 const changed:NativeNestedProject={schemaVersion:1,root:'changed',compositions:{}};nestedComposition.document=changed
 resolve({schemaVersion:1,status:'converted',project,diagnostics:[],sourceDocument:original,timelineIds:{TL1:'imported'}})
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Текущий проект изменился'));expect(nestedComposition.document).toEqual(changed);expect(nestedHistory.past).toHaveLength(0)
})

it('requires every declared font and keeps simultaneous file reads independent',async()=>{
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[]}});await tick()
 const documentWithFonts={version:1,media:[],fonts:[{family:'First',path:'__proto__'},{family:'Second',path:'/never/open/second.ttf'}]}
 const input=document.querySelector<HTMLInputElement>('input[type=file]')!,file=new File([JSON.stringify(documentWithFonts)],'fonts.json',{type:'application/json'})
 Object.defineProperty(file,'text',{value:async()=>JSON.stringify(documentWithFonts)});Object.defineProperty(input,'files',{configurable:true,value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(document.querySelectorAll('input[type=file]')).toHaveLength(3))
 const button=[...document.querySelectorAll('button')].find(button=>button.textContent?.trim()==='Проверить импорт Concat')!;expect(button.disabled).toBe(true)
 let finishFirst!:(value:ArrayBuffer)=>void,finishSecond!:(value:ArrayBuffer)=>void
 for(const [path,promise] of [['__proto__',new Promise<ArrayBuffer>(done=>{finishFirst=done})],['/never/open/second.ttf',new Promise<ArrayBuffer>(done=>{finishSecond=done})]] as const){
  const fontInput=[...document.querySelectorAll<HTMLInputElement>('input[type=file]')].find(input=>input.getAttribute('aria-label')===`Файл шрифта Concat ${path}`)!,font=new File(['font'],'font.ttf')
  Object.defineProperty(font,'arrayBuffer',{value:()=>promise});Object.defineProperty(fontInput,'files',{configurable:true,value:[font]});fontInput.dispatchEvent(new Event('change',{bubbles:true}))
 }
 finishSecond(new Uint8Array([4,5,6]).buffer);await tick();expect(button.disabled).toBe(true)
 finishFirst(new Uint8Array([1,2,3]).buffer);await vi.waitFor(()=>expect(button.disabled).toBe(false))
 click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenCalledWith(documentWithFonts,{},expect.any(AbortSignal),'session',expect.objectContaining({['__proto__']:expect.anything(),'/never/open/second.ttf':{data:'BAUG'}})))
 const bindings=vi.mocked(importConcatProject).mock.calls[0]![4]!;expect(Object.hasOwn(bindings,'__proto__')).toBe(true);expect(bindings['__proto__']).toEqual({data:'AQID'})
})
