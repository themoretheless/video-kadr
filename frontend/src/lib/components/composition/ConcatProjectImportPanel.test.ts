import {readFileSync,writeFileSync} from 'node:fs'
// @ts-expect-error Svelte client entry is required for mounted components under Vitest.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {importConcatProject,importConcatProjectWithSavedStills,getCompositionProjects,fillConcatMediaSlot,evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Panel from './ConcatProjectImportPanel.svelte'
vi.mock('$lib/api',()=>({importConcatProject:vi.fn(),importConcatProjectWithSavedStills:vi.fn(),getCompositionProjects:vi.fn(),fillConcatMediaSlot:vi.fn(),evaluateNestedFrame:vi.fn()}))
const original={version:1,name:'Original cut',media:[{id:'m1',name:'Camera.mov',kind:'video',path:'/never/open-this.mov'}],activeTimelineId:'TL1',timelines:[{id:'TL1',name:'Main',video:{width:16,height:16,rateNum:4,rateDen:1},tracks:[{id:'T1'}],clips:[{id:'clip',kind:'video',trackId:'T1',mediaId:'m1',start:1,duration:1,videoEffects:[{id:'concat.exposure',keys:{stops:[{at:0,value:0},{at:1,value:4,ease:[0.42,-0.5,0.58,1]}]}}]}]}]}
const source={id:'source',kind:'video' as const,durationTicks:1000000,width:16,height:16,hasAudio:false}
const project:NativeNestedProject={schemaVersion:1,root:'imported',compositions:{imported:{content:{schemaVersion:1,timeBase:1000000,canvas:{width:16,height:16,fpsMilli:4000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}},sources:{},tracks:[]},instances:[]}}}
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.clearAllMocks();vi.mocked(getCompositionProjects).mockResolvedValue([]);vi.mocked(importConcatProjectWithSavedStills).mockResolvedValue({schemaVersion:1,status:'converted',project,diagnostics:[],sourceDocument:original,timelineIds:{TL1:'imported'}});vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});vi.mocked(importConcatProject).mockResolvedValue({schemaVersion:1,status:'converted',project,diagnostics:[],sourceDocument:original,timelineIds:{TL1:'imported'}})})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();document.body.innerHTML=''})
function click(text:string){const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===text)!;expect(button).toBeDefined();button.click()}
async function read(value:unknown=original){const input=document.querySelector<HTMLInputElement>('input[type=file]')!;const file=new File([JSON.stringify(value)],'concat.json',{type:'application/json'});Object.defineProperty(file,'text',{value:async()=>JSON.stringify(value)});Object.defineProperty(input,'files',{configurable:true,value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(document.querySelector('select[aria-label="Источник Concat m1"]')).not.toBeNull());await tick();const select=document.querySelector<HTMLSelectElement>('select[aria-label="Источник Concat m1"]')!;select.value='source';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()}
it('loads a file, binds registered media, reviews and adopts with undo/redo',async()=>{
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Library.mov',source}]}});await tick();await read();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenCalledWith(original,{m1:source},expect.any(AbortSignal),'session'));await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'));expect(nestedComposition.document).toBeNull()
 if(process.env.CONCAT_UI_IMPORT_REQUEST_OUTPUT){const[document,mediaBindings]=vi.mocked(importConcatProject).mock.calls[0]!;writeFileSync(process.env.CONCAT_UI_IMPORT_REQUEST_OUTPUT,JSON.stringify({schemaVersion:1,document,mediaBindings,preserveMediaIds:vi.mocked(importConcatProject).mock.calls[0]![5]??false}))}
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

it('fills a source-document slot through Rust, undoes staging and imports its registered replacement',async()=>{
 const slot={...original,media:[{...original.media[0],placeholder:true}]}
 const replacement={label:'New image',source:{id:'image-source',kind:'image' as const,durationTicks:0,width:32,height:16,hasAudio:false}}
 const filled={...slot,media:[{...slot.media[0],placeholder:false,name:'New image',kind:'image'}],timelines:slot.timelines.map(timeline=>({...timeline,clips:timeline.clips.map(clip=>({...clip,kind:'image',sourceStart:0}))}))}
 const freshSource={...replacement.source,width:64,height:48}
 vi.mocked(fillConcatMediaSlot).mockResolvedValue({document:filled,source:freshSource})
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Old',source},replacement]}});await tick();await read(slot)
 async function fill(){const select=document.querySelector<HTMLSelectElement>('select[aria-label="Заполнение слота Concat m1"]')!;select.value='image-source';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();document.querySelector<HTMLButtonElement>('button[aria-label="Заполнить слот Concat m1"]')!.click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Слот заполнен в исходном документе'))}
 await fill();expect(fillConcatMediaSlot).toHaveBeenCalledWith(slot,'m1',replacement,expect.any(AbortSignal),'session');expect(nestedHistory.past).toHaveLength(0)
 click('Отменить заполнение слота');await tick();expect(document.querySelector('button[aria-label="Заполнить слот Concat m1"]')).not.toBeNull()
 await fill();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenCalledWith(filled,{m1:freshSource},expect.any(AbortSignal),'session'))
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'));click('Открыть импорт как новый проект');await vi.waitFor(()=>expect(nestedComposition.document).toEqual(project));expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).toBeNull();redoNestedComposition();expect(nestedComposition.document).toEqual(project)
})
it('rejects a late slot fill after the target editor changes',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof fillConcatMediaSlot>>)=>void;vi.mocked(fillConcatMediaSlot).mockReturnValue(new Promise(done=>{resolve=done}))
 const slot={...original,media:[{...original.media[0],placeholder:true}]}
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Old',source}]}});await tick();await read(slot)
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Заполнение слота Concat m1"]')!;select.value='source';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();document.querySelector<HTMLButtonElement>('button[aria-label="Заполнить слот Concat m1"]')!.click();await vi.waitFor(()=>expect(fillConcatMediaSlot).toHaveBeenCalled())
 const changed:NativeNestedProject={schemaVersion:1,root:'changed',compositions:{}};nestedComposition.document=changed;resolve({document:{...slot,media:[{...slot.media[0],placeholder:false}]},source})
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Текущий проект изменился; повторите заполнение'));expect(nestedComposition.document).toEqual(changed);expect(nestedHistory.past).toHaveLength(0)
})

it('undo restores the prior descriptor even when a fill keeps the same library source ID',async()=>{
 const slot={...original,media:[{...original.media[0],placeholder:true}]},fresh={...source,width:32,height:24}
 const filled={...slot,media:[{...slot.media[0],placeholder:false,name:'Fresh'}]}
 vi.mocked(fillConcatMediaSlot).mockResolvedValue({document:filled,source:fresh})
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Library',source}]}});await tick();await read(slot)
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Заполнение слота Concat m1"]')!;select.value='source';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();document.querySelector<HTMLButtonElement>('button[aria-label="Заполнить слот Concat m1"]')!.click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Слот заполнен в исходном документе'))
 click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenLastCalledWith(filled,{m1:fresh},expect.any(AbortSignal),'session'))
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'));click('Отменить заполнение слота');await tick();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenLastCalledWith(slot,{m1:source},expect.any(AbortSignal),'session'))
})

it('reviews separate logical slots bound to the same file and invalidates review when the mode changes',async()=>{
 const doc={...original,media:[{...original.media[0],placeholder:true},{...original.media[0],id:'m2',name:'Other slot',placeholder:true}]}
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Shared file',source}]}});await tick();await read(doc)
 const second=document.querySelector<HTMLSelectElement>('select[aria-label="Источник Concat m2"]')!;second.value=source.id;second.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const mode=document.querySelector<HTMLInputElement>('input[aria-label="Сохранять отдельные ID медиа Concat"]')!;mode.checked=true;mode.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProject).toHaveBeenCalledWith(doc,{m1:source,m2:source},expect.any(AbortSignal),'session',{},true))
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'))
 mode.checked=false;mode.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(document.body.textContent).not.toContain('Таймлайнов: 1');expect(nestedComposition.document).toBeNull()
})

const imageSource={...source,kind:'image' as const,durationTicks:0}
const cutoutDocument={...original,media:[{...original.media[0],kind:'image'}],timelines:original.timelines.map(t=>({...t,clips:t.clips.map(c=>({...c,kind:'image',cutout:{mode:'auto',feather:0}}))}))}
function savedMasks(){return [{id:'private-source',name:'Portrait',revision:7,document:{...project,projectMedia:{logical:{source:{...imageSource,id:'logical'},librarySourceId:'source',name:'Portrait'}},compositions:{imported:{...project.compositions.imported,content:{...project.compositions.imported!.content,sources:{logical:{...imageSource,id:'logical'},copy:{...imageSource,id:'copy'}},metadata:{clipNames:{'saved-clip':'Cut portrait'},mediaVersions:{'saved-clip':{originalSourceId:'logical',copySourceId:'copy',cutout:{sourceId:'logical'}}}},tracks:[{kind:'image',id:'images',name:'Images',clips:[{id:'saved-clip',sourceId:'copy'}]}]}}}}}] as unknown as Awaited<ReturnType<typeof getCompositionProjects>>}
async function chooseSavedMask(){
 click('Обновить сохранённые маски');await vi.waitFor(()=>expect(getCompositionProjects).toHaveBeenCalled());const select=document.querySelector<HTMLSelectElement>('select[aria-label="Сохранённая маска Concat TL1 clip"]')!
 await vi.waitFor(()=>expect(select.options.length).toBe(2));select.value=select.options[1]!.value;select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
}
it('selects a saved logical mask by its physical source and adopts the reviewed import with one undo step',async()=>{
 vi.mocked(getCompositionProjects).mockResolvedValue(savedMasks());nestedComposition.document=project
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Portrait',source:imageSource}]}});await tick();await read(cutoutDocument)
 const check=[...document.querySelectorAll('button')].find(b=>b.textContent==='Проверить импорт Concat')!;expect(check.disabled).toBe(true)
 await chooseSavedMask();expect(check.disabled).toBe(false);click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProjectWithSavedStills).toHaveBeenCalledWith(cutoutDocument,{m1:imageSource},[{projectId:'private-source',compositionId:'imported',sourceClipId:'saved-clip',expectedRevision:7,timelineId:'TL1',clipId:'clip'}],expect.any(AbortSignal),'session',{},false));expect(importConcatProject).not.toHaveBeenCalled()
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'));click('Открыть импорт как новый проект');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(nestedComposition.projectId).toBeNull();undoNestedComposition();expect(nestedComposition.document).toEqual(project);redoNestedComposition();expect(nestedComposition.document).toEqual(project)
})
it('does not publish a saved-mask catalogue after the editor revision changes',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof getCompositionProjects>>)=>void;vi.mocked(getCompositionProjects).mockReturnValue(new Promise(done=>{resolve=done}))
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Portrait',source:imageSource}]}});await tick();await read(cutoutDocument);click('Обновить сохранённые маски');await vi.waitFor(()=>expect(getCompositionProjects).toHaveBeenCalled());nestedComposition.revision=9;resolve(savedMasks())
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Контекст импорта изменился'));expect(document.querySelector<HTMLSelectElement>('select[aria-label="Сохранённая маска Concat TL1 clip"]')!.options).toHaveLength(1);expect(nestedHistory.past).toHaveLength(0)
})
it('keeps the current editor and refuses fallback after saved-mask verification fails',async()=>{
 vi.mocked(getCompositionProjects).mockResolvedValue(savedMasks());vi.mocked(importConcatProjectWithSavedStills).mockRejectedValue(new Error('stale saved cutout revision'));nestedComposition.document=project
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Portrait',source:imageSource}]}});await tick();await read(cutoutDocument);await chooseSavedMask();click('Проверить импорт Concat');await vi.waitFor(()=>expect(document.body.textContent).toContain('stale saved cutout revision'));expect(importConcatProject).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document).toEqual(project)
})
it('discards a verified ready import after the editor revision changes without a document edit',async()=>{
 vi.mocked(getCompositionProjects).mockResolvedValue(savedMasks());let resolve!:(value:Awaited<ReturnType<typeof importConcatProjectWithSavedStills>>)=>void;vi.mocked(importConcatProjectWithSavedStills).mockReturnValue(new Promise(done=>{resolve=done}))
 component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Portrait',source:imageSource}]}});await tick();await read(cutoutDocument);await chooseSavedMask();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProjectWithSavedStills).toHaveBeenCalled());nestedComposition.revision=10;resolve({schemaVersion:1,status:'converted',project,diagnostics:[],sourceDocument:cutoutDocument,timelineIds:{TL1:'imported'}})
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Текущий проект изменился'));expect(nestedHistory.past).toHaveLength(0)
})
const readyCapture=process.env.CONCAT_READY_IMPORT_UI_CAPTURE_INPUT
it.skipIf(!readyCapture)('adopts an actual private RVM/SAM ready-import response with logical mask collisions and undo/redo',async()=>{
 const capture=JSON.parse(readFileSync(readyCapture!,'utf8')) as {sourceProjects:Awaited<ReturnType<typeof getCompositionProjects>>;request:{document:unknown;mediaBindings:Record<string,typeof imageSource>;preserveMediaIds:boolean};references:Parameters<typeof importConcatProjectWithSavedStills>[2];report:Awaited<ReturnType<typeof importConcatProjectWithSavedStills>>}
 vi.mocked(getCompositionProjects).mockResolvedValue(capture.sourceProjects);vi.mocked(importConcatProjectWithSavedStills).mockResolvedValue(capture.report);nestedComposition.document=project
 component=mount(Panel,{target:document.body,props:{token:'session',sources:Object.values(capture.request.mediaBindings).map(source=>({label:source.id,source}))}});await tick()
 const input=document.querySelector<HTMLInputElement>('input[type=file]')!,file=new File([JSON.stringify(capture.request.document)],'ready.json',{type:'application/json'});Object.defineProperty(file,'text',{value:async()=>JSON.stringify(capture.request.document)});Object.defineProperty(input,'files',{configurable:true,value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(document.querySelectorAll('select[aria-label^="Источник Concat "]').length).toBe(Object.keys(capture.request.mediaBindings).length))
 for(const [id,source]of Object.entries(capture.request.mediaBindings)){const select=[...document.querySelectorAll<HTMLSelectElement>('select')].find(s=>s.getAttribute('aria-label')===`Источник Concat ${id}`)!;select.value=source.id;select.dispatchEvent(new Event('change',{bubbles:true}))}await tick()
 const mode=document.querySelector<HTMLInputElement>('input[aria-label="Сохранять отдельные ID медиа Concat"]')!;mode.checked=true;mode.dispatchEvent(new Event('change',{bubbles:true}));await tick();click('Обновить сохранённые маски')
 const target=capture.references[0]!;const select=document.querySelector<HTMLSelectElement>(`select[aria-label="Сохранённая маска Concat ${target.timelineId} ${target.clipId}"]`)!
 const choice=JSON.stringify([target.projectId,target.compositionId??null,target.sourceClipId]);await vi.waitFor(()=>expect([...select.options].some(option=>option.value===choice)).toBe(true));select.value=choice;select.dispatchEvent(new Event('change',{bubbles:true}));await tick();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProjectWithSavedStills).toHaveBeenCalled());expect(vi.mocked(importConcatProjectWithSavedStills).mock.calls[0]![2]).toEqual(capture.references)
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'));click('Открыть импорт как новый проект');await vi.waitFor(()=>expect(nestedComposition.document).toEqual(capture.report.project));expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).toEqual(project);redoNestedComposition();expect(nestedComposition.document).toEqual(capture.report.project)
})
it('clears private mask choices and rejects late conversion when the account changes',async()=>{
 vi.mocked(getCompositionProjects).mockResolvedValue(savedMasks());let resolve!:(value:Awaited<ReturnType<typeof importConcatProjectWithSavedStills>>)=>void;vi.mocked(importConcatProjectWithSavedStills).mockReturnValue(new Promise(done=>{resolve=done}));nestedComposition.projectId='session'
 component=mount(Panel,{target:document.body,props:{get token(){return nestedComposition.projectId},sources:[{label:'Portrait',source:imageSource}]}});await tick();await read(cutoutDocument);await chooseSavedMask();click('Проверить импорт Concat');await vi.waitFor(()=>expect(importConcatProjectWithSavedStills).toHaveBeenCalled())
 const signal=vi.mocked(importConcatProjectWithSavedStills).mock.calls[0]![3]!;nestedComposition.projectId='different-account';await tick();expect(signal.aborted).toBe(true);expect(document.querySelector<HTMLSelectElement>('select[aria-label="Сохранённая маска Concat TL1 clip"]')!.options).toHaveLength(1)
 resolve({schemaVersion:1,status:'converted',project,diagnostics:[],sourceDocument:cutoutDocument,timelineIds:{TL1:'imported'}});await tick();expect(document.body.textContent).not.toContain('Таймлайнов: 1');expect(nestedHistory.past).toHaveLength(0)
})
it('invalidates a reviewed mask when its file changes and clears choices on a failed refresh',async()=>{
 vi.mocked(getCompositionProjects).mockResolvedValue(savedMasks());component=mount(Panel,{target:document.body,props:{token:'session',sources:[{label:'Portrait',source:imageSource},{label:'Other',source:{...imageSource,id:'other'}}]}});await tick();await read(cutoutDocument);await chooseSavedMask();click('Проверить импорт Concat');await vi.waitFor(()=>expect(document.body.textContent).toContain('Таймлайнов: 1'))
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Источник Concat m1"]')!;select.value='other';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(document.body.textContent).not.toContain('Таймлайнов: 1');expect([...document.querySelectorAll('button')].find(b=>b.textContent==='Проверить импорт Concat')!.disabled).toBe(true)
 select.value='source';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();vi.mocked(getCompositionProjects).mockRejectedValue(new Error('access revoked'));click('Обновить сохранённые маски');await vi.waitFor(()=>expect(document.body.textContent).toContain('access revoked'));expect(document.querySelector<HTMLSelectElement>('select[aria-label="Сохранённая маска Concat TL1 clip"]')!.options).toHaveLength(1);expect(nestedHistory.past).toHaveLength(0)
})
