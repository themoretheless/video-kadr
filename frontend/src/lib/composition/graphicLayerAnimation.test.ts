import { expect, it } from 'vitest'
import type { GraphicContent, CompositionAnimatableValue } from './types'
import { graphicShapeChannels, graphicTextChannels } from './graphicLayerProperties'
import { setGraphicChannelKey, graphicChannelAnimation, editGraphicChannelKey, clearGraphicChannel, graphicKeyTick, setGraphicTextKey, editGraphicTextKey } from './graphicLayerAnimation'
for (const kind of ['shape','text'] as const) it(`edits all ${kind} numeric keys while preserving authored curves and unrelated channels`,()=>{
  const channels=kind==='shape'?graphicShapeChannels:graphicTextChannels
  for(const[channel]of channels){
    const original:GraphicContent={kind,parameters:{},animation:{opacity:{mode:'constant',value:25}}}
    const before=JSON.stringify(original)
    let content=setGraphicChannelKey(original,channel,750000,10)
    content=setGraphicChannelKey(content,channel,250000,20)
    let animation=graphicChannelAnimation(content,channel)!
    expect(animation).toMatchObject({mode:'keyframes',track:{timeBase:1000000,keyframes:[{tick:250000,value:20},{tick:750000,value:10}]}})
    content=editGraphicChannelKey(content,channel,0,{tick:-5,value:30,interpolation:{kind:'bezier',p1:[.2,.3],p2:[.7,.8]}})
    content=setGraphicChannelKey(content,channel,750000,40)
    animation=graphicChannelAnimation(content,channel)!
    expect(animation).toMatchObject({track:{keyframes:[{tick:-5,interpolation:{kind:'bezier',p2:[.7,.8]}},{tick:750000,value:40}]}})
    expect(()=>editGraphicChannelKey(content,channel,0,{tick:750000})).toThrow('unique')
    content=editGraphicChannelKey(content,channel,0,null)
    expect(graphicChannelAnimation(content,channel)).toMatchObject({track:{keyframes:[{tick:750000}]}})
    expect(graphicChannelAnimation(clearGraphicChannel(content,channel),channel)).toBeUndefined()
    expect(JSON.stringify(original)).toBe(before)
  }
})
it('retains fractional authored clocks, tangents, extrapolation and source-text holds',()=>{
  const old:CompositionAnimatableValue={mode:'keyframes',track:{timeBase:3,interpolation:'ease_in',extrapolation:{before:'loop',after:'ping_pong'},keyframes:[{tick:1,value:12,tangents:{incoming:{x:-.2,y:-1,mode:'free'},outgoing:{x:.3,y:2,mode:'free'},continuity:'broken'}}]}}
  const content:GraphicContent={kind:'text',parameters:{text:'A'},animation:{size:old},sourceTextAnimation:{timeBase:3,keyframes:[{tick:-1,value:'Б'}]}}
  const changed=setGraphicChannelKey(content,'size',333333,20)
  expect(graphicChannelAnimation(changed,'size')).toMatchObject({track:{timeBase:3,extrapolation:old.track.extrapolation,keyframes:[{tick:1,value:20,tangents:old.track.keyframes[0]!.tangents}]}})
  expect(graphicKeyTick(9007199254740991,1)).toBe(9007199255)
  expect(graphicKeyTick(-1,1000)).toBe(0)
  expect(graphicKeyTick(-500000,3)).toBe(-2)
  expect(graphicKeyTick(-333333,3)).toBe(-1)
  expect(setGraphicChannelKey({kind:'shape',parameters:{}},'opacity',-500000,35)).toMatchObject({animation:{opacity:{track:{keyframes:[{tick:-500000,value:35}]}}}})
  expect(setGraphicTextKey({kind:'text',parameters:{}},-500000,'Привет')).toMatchObject({sourceTextAnimation:{keyframes:[{tick:-500000,value:'Привет'}]}})
  let text=setGraphicTextKey(content,500000,'🙂')
  expect(text).toMatchObject({sourceTextAnimation:{timeBase:3,keyframes:[{tick:-1,value:'Б'},{tick:2,value:'🙂'}]}})
  text=editGraphicTextKey(text,1,{tick:4,value:'مرحبا'})
  expect(text).toMatchObject({sourceTextAnimation:{keyframes:[{tick:-1,value:'Б'},{tick:4,value:'مرحبا'}]}})
  expect(()=>editGraphicTextKey(text,0,{tick:4})).toThrow('unique')
  text=editGraphicTextKey(text,1,null);text=editGraphicTextKey(text,0,null)
  expect(text).not.toHaveProperty('sourceTextAnimation')
})
it('changes all extrapolation modes independently while retaining the complete authored envelope',async()=>{
 const {setGraphicExtrapolation}=await import('./graphicLayerAnimation')
 const original:GraphicContent={kind:'shape',parameters:{},animation:{opacity:{mode:'keyframes',track:{timeBase:3,interpolation:'hold',keyframes:[{tick:0,value:0},{tick:2,value:100}]}}}}
 for(const side of ['before','after']as const)for(const mode of ['hold','continue','loop','ping_pong','offset']as const){
  const changed=setGraphicExtrapolation(original,'opacity',side,mode)
  const animation=graphicChannelAnimation(changed,'opacity')!
  expect(animation).toMatchObject({track:{timeBase:3,interpolation:'hold',extrapolation:{before:side==='before'?mode:'hold',after:side==='after'?mode:'hold'},keyframes:[{tick:0,value:0},{tick:2,value:100}]}})
 }
 expect(graphicChannelAnimation(original,'opacity')).not.toHaveProperty('track.extrapolation')
})
it('numeric Bezier edits update explicit handles that otherwise override the segment fields',async()=>{
 const {setGraphicSegmentCurve}=await import('./graphicLayerAnimation')
 const {segmentInterpolation}=await import('./tangents')
 const original:GraphicContent={kind:'shape',parameters:{},animation:{opacity:{mode:'keyframes',track:{timeBase:3,interpolation:'linear',keyframes:[{tick:0,value:0,interpolation:{kind:'bezier',p1:[.3,.3],p2:[.7,.7]},tangents:{incoming:{x:.7,y:.7,mode:'free'},outgoing:{x:.1,y:.2,mode:'free'},continuity:'broken'}},{tick:2,value:100,tangents:{incoming:{x:.8,y:.9,mode:'free'},outgoing:{x:.3,y:.3,mode:'free'},continuity:'broken'}}]}}}}
 const changed=setGraphicSegmentCurve(original,'opacity',0,{kind:'bezier',p1:[.25,.4],p2:[.75,.6]})
 const value=graphicChannelAnimation(changed,'opacity')!
 if(value.mode!=='keyframes')throw new Error('keys')
 expect(segmentInterpolation(value.track.keyframes[0]!,value.track.keyframes[1]!,value.track.interpolation)).toEqual({kind:'bezier',p1:[.25,.4],p2:[.75,.6]})
 expect(graphicChannelAnimation(original,'opacity')).toMatchObject({track:{keyframes:[{tangents:{outgoing:{x:.1,y:.2}}},{}]}})
})
