import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {editProjectFontFile,type NativeNestedProject} from '../api'
import {agentSession} from '../agent/session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,addNestedFontFile,removeNestedFontFile,undoNestedComposition,redoNestedComposition} from './nestedComposition.svelte'
vi.mock('../api',()=>({editProjectFontFile:vi.fn()}))
const resource={family:'Custom',data:'AQID'}
const before:NativeNestedProject={schemaVersion:1,root:'root',fonts:[resource],compositions:{}}
const after:NativeNestedProject={schemaVersion:1,root:'root',compositions:{}}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();nestedComposition.document=before;nestedComposition.renderedOutputs={root:'cached'}})
afterEach(()=>{agentSession.lock=null;closeNestedComposition()})
it('commits file removal once, invalidates outputs and restores the full resource through Undo/Redo',async()=>{
 vi.mocked(editProjectFontFile).mockResolvedValue(after)
 await removeNestedFontFile(0,resource)
 expect(editProjectFontFile).toHaveBeenCalledWith(before,{kind:'remove',index:0})
 expect(nestedComposition.document).toEqual(after);expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document).toEqual(before);expect(nestedComposition.renderedOutputs).toEqual({root:'cached'})
 redoNestedComposition();expect(nestedComposition.document).toEqual(after)
})
it('keeps duplicate adds out of history and rejects stale file selections before HTTP',async()=>{
 vi.mocked(editProjectFontFile).mockResolvedValue(before);await addNestedFontFile('AQID');expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'cached'})
 vi.clearAllMocks();await removeNestedFontFile(0,{family:'Other',data:'BAUG'});expect(editProjectFontFile).not.toHaveBeenCalled();expect(nestedComposition.document).toEqual(before);expect(nestedHistory.error).toContain('изменился')
})
it('refuses a history lock both before sending and when acquired during native validation',async()=>{
 agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};await addNestedFontFile('AQID');expect(editProjectFontFile).not.toHaveBeenCalled();expect(nestedHistory.error).toContain('Held')
 agentSession.lock=null;let finish!:(value:NativeNestedProject)=>void;vi.mocked(editProjectFontFile).mockReturnValue(new Promise(done=>{finish=done}));const pending=removeNestedFontFile(0,resource);await vi.waitFor(()=>expect(editProjectFontFile).toHaveBeenCalled())
 agentSession.lock={owner:'other',reason:'Late lock',expiresAt:Date.now()+60000};finish(after);await pending;expect(nestedComposition.document).toEqual(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedHistory.error).toContain('Late lock')
})
