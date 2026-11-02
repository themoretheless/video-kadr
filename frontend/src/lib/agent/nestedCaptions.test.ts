import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {executeAgentRequest} from './runtime.svelte'
import {agentSession} from './session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,resetNestedHistory} from '../state/nestedComposition.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {importSrtToComposition} from '../state/composition/timeline.svelte'
import {buildCompositionWireDocument} from '../composition/payload'
const tool=(name:string,args:Record<string,unknown>={},access={})=>executeAgentRequest('nested-agent',name,args,access)
const read=async()=>await tool('get_project') as {kind:string;projectHash:string;document:api.NativeNestedProject}
beforeEach(()=>{
 closeNestedComposition();resetNestedHistory();setCompositionStorageForTests(null);resetCompositionForTests()
 importSrtToComposition('1\n00:00:01,000 --> 00:00:02,000\nOne\n\n2\n00:00:02,000 --> 00:00:03,000\nTwo\n\n3\n00:00:04,000 --> 00:00:05,000\nThree')
 const content=JSON.parse(JSON.stringify(buildCompositionWireDocument(compositionState.document)))
 const unused=JSON.parse(JSON.stringify(content))
 for(const track of unused.tracks)for(const clip of track.clips)clip.id=`unused-${clip.id}`
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},unused:{content:unused,instances:[]}}}
 nestedComposition.activeCompositionId='unused';nestedComposition.renderedOutputs={root:'old'}
 vi.spyOn(api,'nestedDurations').mockResolvedValue({root:5_000_000,unused:5_000_000})
})
afterEach(()=>{vi.restoreAllMocks();closeNestedComposition();agentSession.lock=null})
it('reads the visible nested project, merges by IDs, routes history and refuses stale hashes after Undo',async()=>{
 const ordinary=JSON.stringify(compositionState.document),before=await read()
 expect(before.kind).toBe('nested');expect(before.projectHash).toMatch(/^[a-f0-9]{64}$/)
 expect((await read()).projectHash).toBe(before.projectHash)
 const ids=before.document.compositions.unused!.content.tracks[0]!.clips.slice(1).map(clip=>clip.id)
 const result=await tool('merge_captions',{expectedProjectHash:before.projectHash,layer_ids:ids.reverse()}) as {projectHash:string}
 expect(result.projectHash).not.toBe(before.projectHash)
 expect(nestedHistory.past).toHaveLength(1)
 expect(await tool('get_history')).toMatchObject({kind:'nested',undoCount:1,redoCount:0})
 await expect(tool('merge_captions',{expectedProjectHash:before.projectHash,layer_ids:ids})).rejects.toThrow('Project changed')
 await tool('undo')
 expect(nestedComposition.document).toEqual(before.document)
 expect((await read()).projectHash).not.toBe(before.projectHash)
 await expect(tool('restyle_captions',{expectedProjectHash:before.projectHash,font_size_px:70})).rejects.toThrow('Project changed')
 await tool('redo');expect(nestedHistory.past).toHaveLength(1)
 expect(JSON.stringify(compositionState.document)).toBe(ordinary)
})
it('restyles the authored project once and rejects unsupported nested tools and ordinary revision guards',async()=>{
 const snapshot=await read()
 expect(await tool('restyle_captions',{expectedProjectHash:snapshot.projectHash,font_size_px:66})).toMatchObject({captions:6,restyled:6,kind:'nested'})
 expect(nestedHistory.past).toHaveLength(1)
 await expect(tool('apply_batch',{expectedRevision:agentSession.revision,operations:[]})).rejects.toThrow('expectedProjectHash')
 await expect(tool('restyle_captions',{expectedRevision:agentSession.revision,font_size_px:66})).rejects.toThrow('expectedProjectHash')
})
it('rechecks locks and connection admission after native validation before any commit',async()=>{
 const before=await read(),identity=JSON.stringify(nestedComposition.document)
 let release!:(value:Record<string,number>)=>void
 vi.mocked(api.nestedDurations).mockImplementation(()=>new Promise(resolve=>{release=resolve}))
 const pending=tool('restyle_captions',{expectedProjectHash:before.projectHash,font_size_px:77})
 await vi.waitFor(()=>expect(api.nestedDurations).toHaveBeenCalled())
 agentSession.lock={owner:'someone-else',reason:'Review',expiresAt:Date.now()+60_000}
 release({root:5_000_000});await expect(pending).rejects.toThrow('История')
 expect(JSON.stringify(nestedComposition.document)).toBe(identity);expect(nestedHistory.past).toHaveLength(0)
})
it('refuses a disconnected no-op even though the document would remain identical',async()=>{
 const before=await read()
 let active=true,release!:(value:Record<string,number>)=>void
 vi.mocked(api.nestedDurations).mockImplementation(()=>new Promise(resolve=>{release=resolve}))
 const pending=tool('restyle_captions',{expectedProjectHash:before.projectHash,font_size_px:48},{isActive:()=>active})
 await vi.waitFor(()=>expect(api.nestedDurations).toHaveBeenCalled())
 active=false;release({root:5_000_000})
 await expect(pending).rejects.toThrow('disconnected')
 expect(nestedHistory.past).toHaveLength(0)
})
