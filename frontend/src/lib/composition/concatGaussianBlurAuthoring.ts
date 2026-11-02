import type { WireVideoEffect,WireAnimatableValue } from './types'
import { sampleAnimatableValue } from './keyframes'
export type GaussianBlurEffect=Extract<WireVideoEffect,{kind:'concat_gaussian_blur'}>
function validTime(timeUs:number){if(!Number.isSafeInteger(timeUs)||timeUs<0)throw new Error('Курсор должен находиться после начала клипа')}
function validRadius(radius:number){if(!Number.isFinite(radius)||radius<1||radius>50)throw new Error('Радиус должен быть от 1 до 50 px')}
export function sampleGaussianBlurRadius(effect:GaussianBlurEffect,timeUs:number):number {
  validTime(timeUs)
  return effect.animation?sampleAnimatableValue(effect.animation,timeUs):effect.parameters.radius??10
}
export function keyGaussianBlurRadius(effect:GaussianBlurEffect,timeUs:number,radius=sampleGaussianBlurRadius(effect,timeUs)):GaussianBlurEffect {
  validTime(timeUs);validRadius(radius)
  if(effect.concatKeys?.length)throw new Error('Исходные ключи Concat нужно редактировать или удалить явно')
  const previous=effect.animation
  const timeBase=previous?.mode==='keyframes'?previous.track.timeBase:1000000
  const tick=Math.round(timeUs*timeBase/1000000)
  if(!Number.isSafeInteger(timeBase)||timeBase<=0||!Number.isSafeInteger(tick))throw new Error('Невозможно представить время ключа')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const existing=points.find(p=>p.tick===tick)
  const prior=points.filter(p=>p.tick<tick).sort((a,b)=>a.tick-b.tick).at(-1)
  const shape=existing??(prior?.interpolation?{interpolation:prior.interpolation}:{})
  const keyframes=[...points.filter(p=>p.tick!==tick),{...shape,tick,value:radius}].sort((a,b)=>a.tick-b.tick)
  const animation:WireAnimatableValue={mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase,interpolation:'linear',keyframes}}
  return {...effect,animation}
}
/** An existing ride authors at the cursor; unkeyed radius keeps constant authoring. */
export function setGaussianBlurRadius(effect:GaussianBlurEffect,timeUs:number,radius:number):GaussianBlurEffect {
  validTime(timeUs);validRadius(radius)
  if(effect.concatKeys?.length)throw new Error('Исходные ключи Concat нужно редактировать или удалить явно')
  if(effect.animation?.mode==='keyframes')return keyGaussianBlurRadius(effect,timeUs,radius)
  if(effect.animation?.mode==='constant')return {...effect,animation:{...effect.animation,value:radius}}
  return {...effect,parameters:{...effect.parameters,radius}}
}
