import {readFileSync} from 'node:fs'
import {beforeEach,expect,it,vi} from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import * as api from '$lib/api'
import {analyseNestedImageCutout,closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from './nestedComposition.svelte'
vi.mock('$lib/api',()=>({getNestedCompositionProject:vi.fn(),analyseNestedCutout:vi.fn(),evaluateNestedFrame:vi.fn()}))
beforeEach(()=>{closeNestedComposition();vi.resetAllMocks()})
function setup(){const document=structuredClone(fixture.createdTimeline) as unknown as api.NativeNestedProject;Object.assign(nestedComposition,{projectId:'saved',revision:7,name:'Saved',document,activeCompositionId:document.root});const dto={id:'saved',name:'Saved',schemaVersion:2,mode:'composition',revision:7,sourceIds:[],createdAt:0,updatedAt:0,document} as api.NestedCompositionProjectDto;vi.mocked(api.getNestedCompositionProject).mockResolvedValue(structuredClone(dto));return dto}
function wider(document:api.NativeNestedProject):api.NativeNestedProject{const id=document.root,node=document.compositions[id]!;return {...document,compositions:{...document.compositions,[id]:{...node,content:{...node.content,canvas:{...node.content.canvas,width:node.content.canvas.width+2}}}}}}
it('commits a checked server revision once and preserves local undo/redo',async()=>{
 const saved=setup(),updated=structuredClone(saved);updated.revision=8;updated.document=wider(updated.document)
 vi.mocked(api.analyseNestedCutout).mockResolvedValue(updated)
 expect(await analyseNestedImageCutout('token',saved.document.root,'image','person',0.01)).toBe(true)
 expect(api.analyseNestedCutout).toHaveBeenCalledWith('saved',saved.document.root,'image',7,'person',0.01,'token',undefined)
 expect(nestedComposition.revision).toBe(8);expect(nestedHistory.past).toHaveLength(1)
 undoNestedComposition();expect(nestedComposition.document).toEqual(saved.document);expect(nestedComposition.revision).toBe(8)
 redoNestedComposition();expect(nestedComposition.document).toEqual(updated.document)
})
it('refuses unsaved changes and stale server revision before analysis',async()=>{
 const saved=setup();nestedComposition.document=wider(nestedComposition.document!)
 expect(await analyseNestedImageCutout('token',saved.document.root,'image','auto',0.01)).toBe(false);expect(api.analyseNestedCutout).not.toHaveBeenCalled();expect(nestedHistory.error).toContain('Сначала сохраните')
 setup();vi.mocked(api.getNestedCompositionProject).mockResolvedValue({...saved,revision:8})
 expect(await analyseNestedImageCutout('token',saved.document.root,'image','auto',0.01)).toBe(false);expect(api.analyseNestedCutout).not.toHaveBeenCalled()
})
it('does not adopt a late result after switching project or cancelling',async()=>{
 const saved=setup();let finish!:(v:api.NestedCompositionProjectDto)=>void
 vi.mocked(api.analyseNestedCutout).mockReturnValue(new Promise(resolve=>{finish=resolve}))
 const pending=analyseNestedImageCutout('token',saved.document.root,'image','object',0)
 await vi.waitFor(()=>expect(api.analyseNestedCutout).toHaveBeenCalled())
 nestedComposition.projectId='other';finish({...saved,revision:8});expect(await pending).toBe(false);expect(nestedHistory.past).toHaveLength(0)
 setup();const cancel=new AbortController();cancel.abort();expect(await analyseNestedImageCutout('token',saved.document.root,'image','auto',0,cancel.signal)).toBe(false)
})

it.runIf(!!process.env.CONCAT_LOGICAL_CUTOUT_HISTORY_INPUT)('adopts a real logical RVM reanalysis response with one Undo/Redo and preserved physical bindings',async()=>{
 const captured=JSON.parse(readFileSync(process.env.CONCAT_LOGICAL_CUTOUT_HISTORY_INPUT!,'utf8')) as {before:api.NestedCompositionProjectDto;after:api.NestedCompositionProjectDto}
 const {before,after}=captured,root=before.document.root
 Object.assign(nestedComposition,{projectId:before.id,revision:before.revision,name:before.name,document:structuredClone(before.document),activeCompositionId:root,renderedOutputs:{[root]:'old movie'}})
 vi.mocked(api.getNestedCompositionProject).mockResolvedValue(before);vi.mocked(api.analyseNestedCutout).mockResolvedValue(after)
 expect(await analyseNestedImageCutout('token',root,'clip','person',0.03)).toBe(true)
 expect(nestedComposition.document).toEqual(after.document);expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.renderedOutputs).toEqual({})
 expect(nestedComposition.document?.projectMedia).toEqual(before.document.projectMedia)
 expect(nestedComposition.document?.projectMedia?.slot?.librarySourceId).toBe('physical')
 expect(after.sourceIds).toContain('physical');expect(after.sourceIds).not.toContain('slot')
 const oldPair=before.document.compositions[root]!.content.metadata!.mediaVersions!.clip!,newPair=after.document.compositions[root]!.content.metadata!.mediaVersions!.clip!
 expect(newPair.originalSourceId).toBe('slot');expect(newPair.copySourceId).not.toBe(oldPair.copySourceId)
 expect(api.analyseNestedCutout).toHaveBeenCalledWith(before.id,root,'clip',before.revision,'person',0.03,'token',undefined,oldPair.cutout!.correction)
 undoNestedComposition();expect(nestedComposition.document).toEqual(before.document);expect(nestedComposition.revision).toBe(after.revision)
 redoNestedComposition();expect(nestedComposition.document).toEqual(after.document);expect(nestedHistory.past).toHaveLength(1)
})
