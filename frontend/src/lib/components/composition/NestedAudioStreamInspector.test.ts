// @ts-expect-error Mounted tests require Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {getSourceAudioStreams,evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedAudioStreamInspector.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn(),getSourceAudioStreams:vi.fn(),getAudioDspCatalog:vi.fn(async()=>({schemaVersion:1,effects:[{id:'concat.echo',nativeExport:true,name:'Echo',params:[{id:'delay',name:'Delay',min:0.05,max:1,default:0.25,unit:'seconds',choices:[]}]}]}))}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{sources:{source:{hasAudio:true}},tracks:[{kind:'video',id:'v',name:'Video',locked:false,clips:[{id:'clip',sourceId:'source',sourceAudioEnabled:true,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},dspEffects:[{id:'concat.echo',params:{delay:0.25}}]}]}]},instances:[]}}} as unknown as NativeNestedProject;nestedComposition.renderedOutputs={root:'stale'};vi.clearAllMocks()})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();document.body.innerHTML=''})

it('edits stream choice and resets to default through undo/redo',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 const clip=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!}
 const input=document.querySelector<HTMLInputElement>('input[aria-label="Индекс аудиопотока"]')!;expect(input).not.toBeNull()
 input.value='2';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 expect(clip().audioStream).toBe(2);expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();expect(clip().audioStream).toBeUndefined();expect(nestedComposition.renderedOutputs).toEqual({root:'stale'});redoNestedComposition();expect(clip().audioStream).toBe(2);await tick()
 input.value='';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(Object.hasOwn(clip(),'audioStream')).toBe(false)
})
it('rejects malformed stream indices before committing',async()=>{
 component=mount(Inspector,{target:document.body});await tick();const input=document.querySelector<HTMLInputElement>('input')!
 for(const value of ['-1','1.5','4294967296']){input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await tick()}
 expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
})
it('keeps the document on failed backend validation and disables locked clips',async()=>{
 component=mount(Inspector,{target:document.body});await tick();vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('invalid composition'));const input=document.querySelector<HTMLInputElement>('input')!;input.value='2';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.error).toBe('invalid composition'));expect(nestedHistory.past).toEqual([]);expect(nestedComposition.renderedOutputs).toEqual({root:'stale'});const locked=JSON.parse(JSON.stringify(nestedComposition.document));locked.compositions.root.content.tracks[0].locked=true;nestedComposition.document=locked;await tick();expect(input.disabled).toBe(true)
})

it('selects an audio clip independently of embedded video audio',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const content=project.compositions.root.content
 content.tracks.push({kind:'audio',id:'a',name:'Audio',locked:false,clips:[{id:'sound',sourceId:'source',audioStream:1,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},gain:{mode:'constant',value:1},pan:{mode:'constant',value:0},fadeInTicks:0,fadeOutTicks:0}]} );nestedComposition.document=project
 component=mount(Inspector,{target:document.body});await tick();const select=document.querySelector<HTMLSelectElement>('select')!;select.value='sound';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const input=document.querySelector<HTMLInputElement>('input')!;expect(input.value).toBe('1');input.value='0';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const tracks=nestedComposition.document!.compositions.root!.content.tracks
 expect('audioStream' in tracks[1]!.clips[0]!&&tracks[1]!.clips[0]!.audioStream).toBe(0);expect('audioStream' in tracks[0]!.clips[0]!).toBe(false)
})

it('loads named streams and commits the selected absolute index',async()=>{
 vi.mocked(getSourceAudioStreams).mockResolvedValue({sourceId:'source',streams:[{index:2,codec:'aac',channels:2,sampleRate:44100,channelLayout:'stereo',language:'rus',title:'Commentary',default:false}]})
 component=mount(Inspector,{target:document.body});await tick();const button=Array.from(document.querySelectorAll('button')).find(button=>button.textContent?.includes('Загрузить список'))!;button.click();await vi.waitFor(()=>expect(document.querySelector('select[aria-label="Аудиодорожка источника"]')).not.toBeNull())
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Аудиодорожка источника"]')!;expect(select.textContent).toContain('Commentary');expect(select.textContent).toContain('rus');select.value='2';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(getSourceAudioStreams).toHaveBeenCalledWith('source',undefined)
})
it('does not show a catalog arriving after the document closes',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof getSourceAudioStreams>>)=>void;vi.mocked(getSourceAudioStreams).mockReturnValue(new Promise(done=>resolve=done));component=mount(Inspector,{target:document.body});await tick();Array.from(document.querySelectorAll('button')).find(button=>button.textContent?.includes('Загрузить список'))!.click();await vi.waitFor(()=>expect(getSourceAudioStreams).toHaveBeenCalled());closeNestedComposition();resolve({sourceId:'source',streams:[]});await tick();await tick();expect(document.querySelector('select[aria-label="Аудиодорожка источника"]')).toBeNull()
})
