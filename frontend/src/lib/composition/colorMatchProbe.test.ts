import {expect,it} from 'vitest'
import {colorMatchProbe} from './colorMatchProbe'
import type {NativeNestedProject} from '$lib/api'
it('isolates the selected shot, disables only its Lumetri and preserves input',()=>{
 const project={schemaVersion:1,root:'root',compositions:{root:{instances:[{id:'child'}],content:{canvas:{background:{red:1,green:0,blue:0,alpha:1}},tracks:[{kind:'video',id:'v',hidden:true,clips:[{id:'shot',effects:[{kind:'lumetri',parameters:{basic:{exposure:1}}},{kind:'basic_grade',parameters:{contrast:22}}]},{id:'other'}]},{kind:'color',id:'background'}]}}}} as unknown as NativeNestedProject
 const probe=colorMatchProbe(project,'root','v','shot',0)
 const target=probe.compositions.root!
 expect(target.instances).toEqual([]);expect(target.content.tracks).toHaveLength(1)
 expect(target.content.canvas.background.alpha).toBe(0)
 const track=target.content.tracks[0]!;if(track.kind!=='video')throw new Error('video')
 expect(track.hidden).toBe(false);expect(track.clips).toHaveLength(1)
 expect(track.clips[0]!.effects[0]).toEqual({kind:'lumetri',parameters:{basic:{exposure:1},enabled:false}})
 expect(track.clips[0]!.effects[1]).toEqual({kind:'basic_grade',parameters:{contrast:22}})
 expect(project.compositions.root!.instances).toHaveLength(1)
 expect(()=>colorMatchProbe(project,'root','v','shot',1)).toThrow()
})
