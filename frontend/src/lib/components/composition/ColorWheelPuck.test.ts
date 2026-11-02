// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import Puck from './ColorWheelPuck.svelte'
let component:ReturnType<typeof mount>|undefined
const scope={revision:1}
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.restoreAllMocks()})
function setup(x=0,y=0,disabled=false){const oncommit=vi.fn();component=mount(Puck,{target:document.body,props:{x,y,disabled,label:'Wheel',scope,oncommit}});return oncommit}
function button(){return document.querySelector('button')!}
function pointer(type:string,x:number,y:number,id=7){const event=new MouseEvent(type,{bubbles:true,clientX:x,clientY:y,button:0});Object.defineProperty(event,'pointerId',{value:id});button().dispatchEvent(event)}
function geometry(){vi.spyOn(button(),'getBoundingClientRect').mockReturnValue({left:0,top:0,width:100,height:100,right:100,bottom:100,x:0,y:0,toJSON:()=>({})});Object.defineProperty(button(),'setPointerCapture',{configurable:true,value:vi.fn()})}
it('shows drafts, clamps to the unit disc and commits a whole pointer gesture once',async()=>{
 const commit=setup(0.1,0.2);await tick();geometry();pointer('pointerdown',50,50);pointer('pointermove',100,0);await tick();expect(commit).not.toHaveBeenCalled();expect(document.querySelector('.puck')?.getAttribute('style')).toContain('85.355');pointer('pointerup',100,0);await tick();expect(commit).toHaveBeenCalledTimes(1);expect(commit.mock.calls[0]![0]).toBeCloseTo(Math.SQRT1_2);expect(commit.mock.calls[0]![1]).toBeCloseTo(Math.SQRT1_2);expect(commit.mock.calls[0]![2]).toBe(scope);pointer('pointerup',0,0);expect(commit).toHaveBeenCalledTimes(1)
})
it.each(['pointercancel','lostpointercapture'])('discards %s and ignores unrelated pointer cancellation',async(type)=>{
 const commit=setup();await tick();geometry();pointer('pointerdown',60,30);pointer(type,0,0,99);await tick();expect(document.querySelector('.puck')?.getAttribute('style')).toContain('60%');pointer(type,60,30);pointer('pointerup',90,10);expect(commit).not.toHaveBeenCalled()
})
it('supports keyboard fine/coarse movement and centering, and respects disabled',async()=>{
 const commit=setup(0.2,-0.3);await tick();button().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}));expect(commit.mock.calls[0]).toEqual([0.2,-0.29,scope]);button().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',shiftKey:true,bubbles:true}));expect(commit.mock.calls[1]![0]).toBeCloseTo(0.3);button().dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true}));expect(commit.mock.calls[2]).toEqual([0,0,scope]);await unmount(component!);component=undefined;document.body.innerHTML='';const blocked=setup(0,0,true);await tick();geometry();pointer('pointerdown',100,0);pointer('pointerup',100,0);button().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}));expect(blocked).not.toHaveBeenCalled()
})
it('Escape discards an active gesture without committing',async()=>{const commit=setup();await tick();geometry();pointer('pointerdown',80,10);button().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));pointer('pointerup',80,10);expect(commit).not.toHaveBeenCalled()})
it.each(['replaceScope','lock','seek','logout','switchComposition']as const)('discards a gesture after %s',async(action)=>{
 const {default:Harness}=await import('./fixtures/ColorWheelPuckHarness.svelte');const commit=vi.fn();const harness=mount(Harness,{target:document.body,props:{oncommit:commit}});component=harness;await tick();geometry();pointer('pointerdown',80,10);harness[action]();await tick();pointer('pointerup',80,10);expect(commit).not.toHaveBeenCalled()
})
