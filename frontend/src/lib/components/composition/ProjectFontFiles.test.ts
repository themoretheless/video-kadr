// @ts-expect-error Client entry required for mounted Svelte components.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {editProjectFontFile,type NativeNestedProject} from '$lib/api'
import {agentSession} from '$lib/agent/session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition} from '$lib/state/nestedComposition.svelte'
import Files from './ProjectFontFiles.svelte'
vi.mock('$lib/api',()=>({editProjectFontFile:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const project:NativeNestedProject={schemaVersion:1,root:'root',compositions:{}}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();nestedComposition.document=project})
afterEach(async()=>{if(component)await unmount(component);component=undefined;agentSession.lock=null;closeNestedComposition();document.body.innerHTML=''})
async function begin(){component=mount(Files,{target:document.body});await tick();let finish!:(value:ArrayBuffer)=>void;const promise=new Promise<ArrayBuffer>(done=>{finish=done}),file=new File(['font'],'font.ttf');Object.defineProperty(file,'arrayBuffer',{value:()=>promise});const input=document.querySelector<HTMLInputElement>('input')!;Object.defineProperty(input,'files',{configurable:true,value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}));await tick();return finish}
it('reports busy state acquired during file reading instead of silently dropping the add',async()=>{
 const finish=await begin();nestedHistory.busy=true;finish(new Uint8Array([1,2,3]).buffer);await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Редактор занят'));expect(editProjectFontFile).not.toHaveBeenCalled();expect(nestedComposition.document).toEqual(project)
})
it('rejects a document change during file reading before sending font data',async()=>{
 const finish=await begin();const changed={...project,luts:[]};nestedComposition.document=changed;await tick();finish(new Uint8Array([1,2,3]).buffer);await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Проект изменился'));expect(editProjectFontFile).not.toHaveBeenCalled();expect(nestedComposition.document).toEqual(changed)
})
