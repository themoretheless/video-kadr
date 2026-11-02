// @ts-expect-error Tests use the Svelte client entry under SSR resolution.
import{mount,tick,unmount}from'../../../../node_modules/svelte/src/index-client.js'
import{afterEach,beforeEach,expect,it,vi}from'vitest'
import{evaluateNestedFrame,sampleConcatCurve,type NativeNestedProject}from'$lib/api'
import{closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition}from'$lib/state/nestedComposition.svelte'
import Inspector from'./NestedConcatAdjustInspector.svelte'
import fixture from'../../../../../backend/tests/fixtures/concat-color-edit.json'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn(),sampleConcatCurve:vi.fn(),sampleToneCurve:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});vi.mocked(sampleConcatCurve).mockResolvedValue(Array.from({length:256},(_,i)=>i/255));const base=structuredClone(fixture.cases[0]!.original)as unknown as NativeNestedProject;const root=base.compositions.root!;const tracks=root.content.tracks;const track=tracks[0]!;if(track.kind!=='video')throw new Error('video');nestedComposition.document={...base,compositions:{...base.compositions,root:{...root,content:{...root.content,tracks:[{...track,clips:track.clips.map(c=>({...c,effects:[]}))},...tracks.slice(1)]}}}};nestedComposition.timeUs=1500000;nestedComposition.renderedOutputs={root:'old-movie'}})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function button(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();return b!}
function grade(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='concat_adjust')throw new Error('adjust');return e}
async function add(){button('Добавить Concat Adjust').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()}
it('exposes all source scalar controls, wheels and curves; updates one effect and undoes the whole panel',async()=>{
 const before=JSON.stringify(nestedComposition.document);component=mount(Inspector,{target:document.body,props:{token:'first'}});await tick();await add();
 expect(document.querySelectorAll('input[aria-label^="clip Concat Adjust "][type="number"]')).toHaveLength(22);expect(document.querySelectorAll('[aria-label$="цветовое колесо"]')).toHaveLength(3);expect(document.querySelectorAll('svg[aria-label="Кривая Яркость (luma)"]')).toHaveLength(1);
 expect([...document.querySelectorAll('button')].filter(b=>b.textContent?.trim()==='Добавить Concat Adjust')).toHaveLength(1);
 const brightness=document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Adjust brightness"]')!;brightness.value='12';brightness.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(grade().parameters.brightness).toBe(12);await tick();button('Добавить точку').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));expect(grade().parameters.luma).toEqual([[0,0],[1,1],[0.5,0.5]]);expect(grade().parameters.brightness).toBe(12);await tick();
 const key=document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Adjust gain общий ключ"]')!;key.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(grade().animation?.['gain.m']?.mode).toBe('keyframes');expect(grade().parameters.luma).toHaveLength(3);
 for(let i=0;i<4;i++)undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-movie'});
})
it('rejects late scalar validation after account scope changes',async()=>{
 const{default:Harness}=await import('./fixtures/ConcatAdjustHarness.svelte');const harness=mount(Harness,{target:document.body});component=harness;await tick();await add();const before=JSON.stringify(nestedComposition.document);let resolve!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockReturnValueOnce(new Promise(done=>resolve=done));const input=document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Adjust exposure"]')!;input.value='1';input.dispatchEvent(new Event('change',{bubbles:true}));await tick();harness.changeSession();await tick();resolve({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(1)
})
