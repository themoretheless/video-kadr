// @ts-expect-error Tests use the Svelte browser entry.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect,it,vi } from 'vitest'
import fixture from '../../../../../backend/tests/fixtures/concat-merge-preview.json'
import * as api from '$lib/api'
import Panel from './NativeSpeechPanel.svelte'
import { state as library } from '$lib/state/store.svelte'
import { nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition } from '$lib/state/nestedComposition.svelte'
vi.mock('$lib/api',()=>({getLocalSpeechStatus:vi.fn(),listLocalSpeechModels:vi.fn(),selectLocalSpeechModel:vi.fn(),importLocalSpeechModel:vi.fn(),removeLocalSpeechModel:vi.fn(),listLocalSpeechJobs:vi.fn(),transcribeLocalSpeech:vi.fn(),observeLocalSpeechJob:vi.fn(),cancelLocalSpeechJob:vi.fn(async()=>({status:'cancelled'})),planSpeechClip:vi.fn(),alignSpeechClip:vi.fn(),evaluateNestedFrame:vi.fn(async()=>({entries:[]}))}))
const transcript:api.LocalSpeechTranscript={language:'ru',modelHash:'a'.repeat(64),sourceStartUs:0,durationUs:11000000,segments:[{startUs:120000,endUs:930001,text:'Пример речи'}]}
it.each(['phrase','word'] as const)('recognizes an explicit library range with %s segmentation and adds captions with one Undo',async(mode)=>{
  closeNestedComposition();vi.clearAllMocks()
  const original=structuredClone(fixture.nativeCaptionPastes[0]!.before) as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline';nestedComposition.timeUs=2000000
  library.library=[{id:'speech',kind:'source',filename:'speech.wav',url:'/files/sources/speech.wav',mediaType:'audio',duration:11,createdAt:0}]
  vi.mocked(api.getLocalSpeechStatus).mockResolvedValue({engine:'whisper.cpp',available:true,model:'tiny',maxDurationUs:600000000})
  const recognized=mode==='word'?{...transcript,segmentation:'word' as const,segments:[{startUs:120000,endUs:500000,text:'Пример'},{startUs:500000,endUs:930001,text:'речи'}]}:transcript
  vi.mocked(api.transcribeLocalSpeech).mockResolvedValue(recognized)
  const target=document.createElement('div');document.body.append(target);const component=mount(Panel,{target,props:{token:'test-token'}})
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try{
    await tick();button('Проверить модель речи').click();await vi.waitFor(()=>expect(target.textContent).toContain('Модель: tiny'))
    const select=target.querySelector<HTMLSelectElement>('[aria-label="Источник распознавания речи"]')!;select.value='speech';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    const segmentationSelect=target.querySelector<HTMLSelectElement>('[aria-label="Разбиение распознанной речи"]')!;segmentationSelect.value=mode;segmentationSelect.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    button('Распознать речь').click();await vi.waitFor(()=>expect(target.querySelector('textarea')?.value).toBe(mode==='word'?'Пример':'Пример речи'))
    expect(api.transcribeLocalSpeech).toHaveBeenCalledWith('test-token',{sourceId:'speech',startUs:0,endUs:11000000,language:'auto',segmentation:mode},expect.any(AbortSignal),expect.any(Function))
    expect(nestedHistory.past).toHaveLength(0)
    const placement=target.querySelector<HTMLInputElement>('[aria-label="Начало распознанных субтитров"]')!;placement.value='5';placement.dispatchEvent(new Event('input',{bubbles:true}));await tick()
    const text=target.querySelector<HTMLTextAreaElement>('textarea')!;text.value='Исправленный текст';text.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    button('Добавить распознанные субтитры').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
    const cue=nestedComposition.document!.compositions['new-timeline']!.content.captionTracks!.at(-1)!.clips[0]!
    expect(cue).toMatchObject({timelineStartTick:5120000,timelineEndTick:mode==='word'?5500000:5930001,text:'Исправленный текст'})
    undoNestedComposition();expect(nestedComposition.document).toEqual(original)
  }finally{await unmount(component);target.remove();closeNestedComposition();library.library=[]}
})
it('aborts the waiting UI and never authors a late recognition response after unmount',async()=>{
  closeNestedComposition();vi.clearAllMocks()
  const original=structuredClone(fixture.nativeCaptionPastes[0]!.before) as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline'
  library.library=[{id:'speech',kind:'source',filename:'speech.wav',url:'/files/sources/speech.wav',mediaType:'audio',duration:11,createdAt:0}]
  vi.mocked(api.getLocalSpeechStatus).mockResolvedValue({engine:'whisper.cpp',available:true,model:'tiny',maxDurationUs:600000000})
  let release!:(value:api.LocalSpeechTranscript)=>void
  vi.mocked(api.transcribeLocalSpeech).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
  const target=document.createElement('div');document.body.append(target);const component=mount(Panel,{target,props:{token:'test-token'}})
  try{
    await tick();[...target.querySelectorAll('button')].find(button=>button.textContent==='Проверить модель речи')!.click();await vi.waitFor(()=>expect(target.textContent).toContain('Модель: tiny'))
    const select=target.querySelector<HTMLSelectElement>('[aria-label="Источник распознавания речи"]')!;select.value='speech';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    ;[...target.querySelectorAll('button')].find(button=>button.textContent==='Распознать речь')!.click();await vi.waitFor(()=>expect(release).toBeDefined())
    const signal=vi.mocked(api.transcribeLocalSpeech).mock.calls[0]![2]!
    await unmount(component);expect(signal.aborted).toBe(true);release(transcript);await new Promise(resolve=>setTimeout(resolve,0))
    expect(nestedComposition.document).toEqual(original);expect(nestedHistory.past).toHaveLength(0)
  }finally{target.remove();closeNestedComposition();library.library=[]}
})

it('takes the source range from a selected clip and authors the native returned caption times without manual offset',async()=>{
  closeNestedComposition();vi.clearAllMocks()
  const original=structuredClone(fixture.nativeVersions[0]!.original) as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline'
  library.library=[{id:'source',kind:'source',filename:'source.mov',url:'/files/sources/source.mov',mediaType:'video',duration:10,createdAt:0}]
  vi.mocked(api.getLocalSpeechStatus).mockResolvedValue({engine:'whisper.cpp',available:true,model:'tiny',maxDurationUs:600000000})
  vi.mocked(api.planSpeechClip).mockResolvedValue({sourceId:'source',sourceStartUs:1000000,sourceEndUs:4000000,timelineStartUs:2000000,timelineEndUs:5000000})
  const spoken={...transcript,sourceStartUs:1000000,durationUs:3000000}
  vi.mocked(api.transcribeLocalSpeech).mockResolvedValue(spoken)
  vi.mocked(api.alignSpeechClip).mockResolvedValue({id:'aligned',name:'Aligned',clips:[{id:'aligned-cue',timelineStartTick:3250000,timelineEndTick:3750000,text:'Пример речи'}]})
  const target=document.createElement('div');document.body.append(target);const component=mount(Panel,{target,props:{token:'test-token'}})
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try{
    await tick();button('Проверить модель речи').click();await vi.waitFor(()=>expect(target.textContent).toContain('Модель: tiny'))
    const clip=target.querySelector<HTMLSelectElement>('[aria-label="Клип для распознанной речи"]')!;clip.value='auto-first';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    expect(target.querySelector<HTMLInputElement>('[aria-label="Начало распознавания речи"]')!.disabled).toBe(true)
    button('Распознать речь').click();await vi.waitFor(()=>expect(target.querySelector('textarea')?.value).toBe('Пример речи'))
    expect(api.planSpeechClip).toHaveBeenCalledWith(original,'new-timeline','auto-first')
    expect(api.transcribeLocalSpeech).toHaveBeenCalledWith('test-token',{sourceId:'source',startUs:1000000,endUs:4000000,language:'auto',segmentation:'phrase'},expect.any(AbortSignal),expect.any(Function))
    expect(target.querySelector('[aria-label="Начало распознанных субтитров"]')).toBeNull()
    button('Добавить распознанные субтитры').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
    expect(api.alignSpeechClip).toHaveBeenCalledWith(original,'new-timeline','auto-first','source',spoken)
    expect(nestedComposition.document!.compositions['new-timeline']!.content.captionTracks!.at(-1)!.clips[0]).toMatchObject({timelineStartTick:3250000,timelineEndTick:3750000})
    undoNestedComposition();expect(nestedComposition.document).toEqual(original)
  }finally{await unmount(component);target.remove();closeNestedComposition();library.library=[]}
})

it('offers cancellation outside the disabled fieldset and refuses a late result after cancel',async()=>{
  closeNestedComposition();vi.clearAllMocks()
  const original=structuredClone(fixture.nativeCaptionPastes[0]!.before) as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline'
  library.library=[{id:'speech',kind:'source',filename:'speech.wav',url:'/files/sources/speech.wav',mediaType:'audio',duration:11,createdAt:0}]
  vi.mocked(api.getLocalSpeechStatus).mockResolvedValue({engine:'whisper.cpp',available:true,model:'tiny',maxDurationUs:600000000})
  let release!:(value:api.LocalSpeechTranscript)=>void
  vi.mocked(api.transcribeLocalSpeech).mockImplementationOnce((_token,_input,_signal,onJob)=>new Promise(resolve=>{release=resolve;onJob?.({jobId:'speech-job',status:'running',progress:44.75})}))
  const target=document.createElement('div');document.body.append(target);const component=mount(Panel,{target,props:{token:'test-token'}})
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try{
    await tick();button('Проверить модель речи').click();await vi.waitFor(()=>expect(target.textContent).toContain('Модель: tiny'))
    const select=target.querySelector<HTMLSelectElement>('[aria-label="Источник распознавания речи"]')!;select.value='speech';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    button('Распознать речь').click();await vi.waitFor(()=>expect(release).toBeDefined())
    expect(target.querySelector<HTMLProgressElement>('[aria-label="Ход распознавания речи"]')?.value).toBe(44.75)
    expect(button('Отменить распознавание').closest('fieldset')).toBeNull()
    button('Отменить распознавание').click()
    expect(vi.mocked(api.transcribeLocalSpeech).mock.calls[0]![2]!.aborted).toBe(true)
    release(transcript);await vi.waitFor(()=>expect(target.querySelector('[role="alert"]')).not.toBeNull())
    expect(target.querySelector('textarea[aria-label^="Текст распознанного"]')).toBeNull();expect(nestedHistory.past).toHaveLength(0)
  }finally{await unmount(component);target.remove();closeNestedComposition();library.library=[]}
})

it.each(['resume','changed','cancel','close'])('reconnects to admitted speech without another recognition and retains cancellation (%s)',async(mode)=>{
  const replaceDocument=mode==='changed'
  closeNestedComposition();vi.clearAllMocks()
  const original=structuredClone(fixture.nativeCaptionPastes[0]!.before) as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline'
  library.library=[{id:'speech',kind:'source',filename:'speech.wav',url:'/files/sources/speech.wav',mediaType:'audio',duration:11,createdAt:0}]
  vi.mocked(api.getLocalSpeechStatus).mockResolvedValue({engine:'whisper.cpp',available:true,model:'tiny',maxDurationUs:600000000})
  vi.mocked(api.transcribeLocalSpeech).mockImplementationOnce(async(_token,_input,_signal,onJob)=>{onJob?.({jobId:'persisted-job',status:'running',progress:44.75});throw new Error('Связь прервалась')})
  vi.mocked(api.observeLocalSpeechJob).mockResolvedValue(transcript)
  const target=document.createElement('div');document.body.append(target);const component=mount(Panel,{target,props:{token:'test-token'}})
  let mounted=true
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try{
    await tick();button('Проверить модель речи').click();await vi.waitFor(()=>expect(target.textContent).toContain('Модель: tiny'))
    const select=target.querySelector<HTMLSelectElement>('[aria-label="Источник распознавания речи"]')!;select.value='speech';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    button('Распознать речь').click();await vi.waitFor(()=>expect(target.textContent).toContain('Связь прервалась'))
    if(mode==='cancel'){
      button('Остановить распознавание').click();await vi.waitFor(()=>expect(api.cancelLocalSpeechJob).toHaveBeenCalledWith('test-token','persisted-job'))
      expect(api.observeLocalSpeechJob).not.toHaveBeenCalled();return
    }
    if(mode==='close'){
      await unmount(component);mounted=false
      expect(api.cancelLocalSpeechJob).toHaveBeenCalledWith('test-token','persisted-job')
      expect(api.observeLocalSpeechJob).not.toHaveBeenCalled();return
    }
    if(replaceDocument)nestedComposition.document=structuredClone(original)
    await tick();button('Повторить наблюдение за распознаванием').click()
    if(replaceDocument){
      await vi.waitFor(()=>expect(target.textContent).toContain('Проект или исходник изменился'))
      expect(api.observeLocalSpeechJob).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
    }else{
      await vi.waitFor(()=>expect(target.querySelector('textarea')?.value).toBe('Пример речи'))
      expect(api.observeLocalSpeechJob).toHaveBeenCalledWith('test-token','persisted-job',expect.any(AbortSignal),expect.any(Function))
      button('Добавить распознанные субтитры').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
    }
    expect(api.transcribeLocalSpeech).toHaveBeenCalledTimes(1)
  }finally{if(mounted)await unmount(component);target.remove();closeNestedComposition();library.library=[]}
})

it('loads persisted speech pages and restores a chosen result without new recognition',async()=>{
  closeNestedComposition();vi.clearAllMocks()
  const original=structuredClone(fixture.nativeCaptionPastes[0]!.before) as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline';nestedComposition.timeUs=2000000
  library.library=[{id:'speech',kind:'source',filename:'speech.wav',url:'/files/sources/speech.wav',mediaType:'audio',duration:11,createdAt:0}]
  const summary={jobId:'stored',sourceId:'speech',status:'done' as const,progress:100,createdAt:1}
  vi.mocked(api.listLocalSpeechJobs).mockResolvedValueOnce({jobs:[summary],nextCursor:'stored'}).mockResolvedValueOnce({jobs:[summary,{...summary,jobId:'older'}],nextCursor:null})
  vi.mocked(api.observeLocalSpeechJob).mockResolvedValue(transcript)
  const target=document.createElement('div');document.body.append(target);const component=mount(Panel,{target,props:{token:'test-token'}})
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try{
    await tick();button('Показать сохранённые распознавания').click();await vi.waitFor(()=>expect(button('speech.wav — готово')).toBeDefined())
    expect(api.listLocalSpeechJobs).toHaveBeenCalledWith('test-token',undefined)
    button('Ещё распознавания').click();await vi.waitFor(()=>expect(target.querySelectorAll('li')).toHaveLength(2))
    expect(api.listLocalSpeechJobs).toHaveBeenLastCalledWith('test-token','stored')
    button('speech.wav — готово').click();await vi.waitFor(()=>expect(target.querySelector('textarea')?.value).toBe('Пример речи'))
    expect(api.observeLocalSpeechJob).toHaveBeenCalledWith('test-token','stored',expect.any(AbortSignal),expect.any(Function))
    expect(api.transcribeLocalSpeech).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
    button('Добавить распознанные субтитры').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
  }finally{await unmount(component);target.remove();closeNestedComposition();library.library=[]}
})
it('suppresses an old recognition response when account changes',async()=>{
 closeNestedComposition();vi.clearAllMocks();nestedComposition.document=structuredClone(fixture.nativeCaptionPastes[0]!.before) as unknown as api.NativeNestedProject;nestedComposition.activeCompositionId='new-timeline';library.library=[{id:'speech',kind:'source',filename:'speech.wav',url:'/files/sources/speech.wav',mediaType:'audio',duration:11,createdAt:0}];vi.mocked(api.getLocalSpeechStatus).mockResolvedValue({engine:'whisper.cpp',available:true,model:'tiny',maxDurationUs:600000000});let finish!:(value:api.LocalSpeechTranscript)=>void;vi.mocked(api.transcribeLocalSpeech).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));const Harness=(await import('./test-fixtures/SpeechOwnerHarness.svelte')).default;const target=document.createElement('div');document.body.append(target);const component=mount(Harness,{target});const button=(name:string)=>Array.from(target.querySelectorAll('button')).find(b=>b.textContent===name)!;
 try{await tick();button('Проверить модель речи').click();await vi.waitFor(()=>expect(target.textContent).toContain('Модель: tiny'));const select=target.querySelector<HTMLSelectElement>('[aria-label="Источник распознавания речи"]')!;select.value='speech';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();button('Распознать речь').click();await vi.waitFor(()=>expect(api.transcribeLocalSpeech).toHaveBeenCalledTimes(1));const signal=vi.mocked(api.transcribeLocalSpeech).mock.calls[0]![2]!;button('Сменить аккаунт').click();await tick();expect(signal.aborted).toBe(true);finish(transcript);await tick();expect(target.querySelector('textarea[aria-label^="Текст распознанного"]')).toBeNull();expect(target.textContent).not.toContain('Модель: tiny');expect(nestedHistory.past).toHaveLength(0)}finally{await unmount(component);target.remove();closeNestedComposition();library.library=[]}
})
