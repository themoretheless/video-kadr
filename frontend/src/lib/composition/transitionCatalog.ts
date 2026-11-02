import source from './transitionDefinitions.json'
export type TransitionParameterValue = {Float:number}|{Choice:number}|{Bool:boolean}|{Color:readonly number[]}|{Vec2:{x:number|null,y:number|null}}
export interface ProceduralTransitionSpec {readonly effect:string;readonly params?:Readonly<Record<string,TransitionParameterValue>>}
export interface TransitionDefinition {readonly id:string;readonly name:string;readonly category:readonly string[];readonly params:readonly {readonly id:string;readonly label:string;readonly default:TransitionParameterValue;readonly kind:unknown}[]}
const concatDefinitions:readonly TransitionDefinition[]=[{id:'concat.glitch-warp',name:'Concat Glitch Warp',category:['Concat','Glitch'],params:[{id:'amount',label:'Сила искажения, %',default:{Float:75},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.glitch-transition',name:'Concat Glitch',category:['Concat','Glitch'],params:[{id:'amount',label:'Сила сбоев, %',default:{Float:70},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.cube',name:'Concat Cube',category:['Concat','Light'],params:[]},{id:'concat.film-burn',name:'Concat Film Burn',category:['Concat','Dissolve'],params:[{id:'heat',label:'Жар, %',default:{Float:70},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.zoom-blur-transition',name:'Concat Zoom Blur',category:['Concat','Zoom'],params:[{id:'strength',label:'Радиальное размытие, %',default:{Float:70},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.light-leak-transition',name:'Concat Light Leak',category:['Concat','Light'],params:[{id:'warmth',label:'Теплота, %',default:{Float:70},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.light-leak-flash',name:'Concat Light Leak Flash',category:['Concat','Light'],params:[{id:'strength',label:'Сила вспышки, %',default:{Float:80},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.zoom-transition',name:'Concat Zoom',category:['Concat','Zoom'],params:[{id:'direction',label:'Направление',default:{Choice:0},kind:{Choice:['In','Out']}}]},{id:'concat.whip-pan',name:'Concat Whip Pan',category:['Concat','Wipe'],params:[{id:'speed',label:'Скорость, %',default:{Float:80},kind:{Float:{min:20,max:100,decimals:0}}}]},{id:'concat.blur-dissolve',name:'Concat Blur Dissolve',category:['Concat','Dissolve'],params:[{id:'strength',label:'Размытие, %',default:{Float:60},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.spin',name:'Concat Spin',category:['Concat','Spin'],params:[{id:'turns',label:'Обороты',default:{Float:1},kind:{Float:{min:0.5,max:2,decimals:2,step:0.25}}}]},{id:'concat.swirl',name:'Concat Swirl',category:['Concat','Spin'],params:[{id:'twist',label:'Скручивание, %',default:{Float:70},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.rgb-split-wipe',name:'Concat RGB Split',category:['Concat','Glitch'],params:[{id:'spread',label:'Разведение каналов, %',default:{Float:60},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.push',name:'Concat Push',category:['Concat','Slide'],params:[{id:'direction',label:'Направление',default:{Choice:0},kind:{Choice:['Влево','Вправо','Вверх','Вниз']}}]},
{id:'concat.slide',name:'Concat Slide',category:['Concat','Slide'],params:[{id:'direction',label:'Направление',default:{Choice:0},kind:{Choice:['Влево','Вправо','Вверх','Вниз']}}]},{id:'concat.shape-wipe',name:'Concat Shape Wipe',category:['Concat','Wipe'],params:[{id:'shape',label:'Форма',default:{Choice:0},kind:{Choice:['Круг','Сердце','Звезда']}},{id:'softness',label:'Мягкость, %',default:{Float:15},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.iris',name:'Concat Iris',category:['Concat','Wipe'],params:[{id:'softness',label:'Мягкость, %',default:{Float:15},kind:{Float:{min:0,max:100,decimals:0}}}]},{id:'concat.clock-wipe',name:'Concat Clock Wipe',category:['Concat','Wipe'],params:[]},{id:'concat.linear-wipe',name:'Concat Linear Wipe',category:['Concat','Wipe'],params:[
 {id:'direction',label:'Направление',default:{Choice:0},kind:{Choice:['Слева направо','Справа налево','Сверху вниз','Снизу вверх']}},
 {id:'softness',label:'Мягкость, %',default:{Float:20},kind:{Float:{min:0,max:100,decimals:0}}},
]}]
export const TRANSITION_DEFINITIONS:readonly TransitionDefinition[]=[...source.definitions as unknown as readonly TransitionDefinition[],...concatDefinitions]
const record=(v:unknown):v is Record<string,unknown>=>typeof v==='object'&&v!==null&&!Array.isArray(v)
const finite=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v)
export function defaultProceduralTransition(effect='cross_dissolve'):ProceduralTransitionSpec {
 const definition=TRANSITION_DEFINITIONS.find(d=>d.id===effect)
 if(!definition)throw new Error('Unknown procedural transition')
 return {effect,params:Object.fromEntries(definition.params.map(p=>[p.id,structuredClone(p.default)]))}
}
export function proceduralTransitionReason(value:unknown):string|null {
 if(!record(value)||typeof value.effect!=='string'||value.effect.length>128||Object.keys(value).some(k=>k!=='effect'&&k!=='params'))return 'Invalid procedural transition spec'
 const definition=TRANSITION_DEFINITIONS.find(d=>d.id===value.effect)
 if(!definition)return 'Unknown procedural transition'
 const params=value.params??{}
 if(!record(params)||Object.keys(params).length>32)return 'Invalid transition parameter budget'
 for(const [id,v]of Object.entries(params)){
  const parameter=definition.params.find(p=>p.id===id)
  if(!parameter||id.length>128||!record(v)||Object.keys(v).length!==1)return 'Unknown or invalid transition parameter'
  const kind=parameter.kind
  let valid=false
  if(record(kind)&&record(kind.Float)){const n=v.Float;valid=finite(n)&&n>=(kind.Float.min as number)&&n<=(kind.Float.max as number)}
  else if(record(kind)&&Array.isArray(kind.Choice))valid=Number.isInteger(v.Choice)&&typeof v.Choice==='number'&&v.Choice>=0&&v.Choice<kind.Choice.length
  else if(kind==='Angle')valid=finite(v.Float)&&Math.abs(v.Float)<=3.4028234663852886e38
  else if(kind==='Bool')valid=typeof v.Bool==='boolean'
  else if(kind==='Color')valid=Array.isArray(v.Color)&&v.Color.length===4&&v.Color.every(n=>finite(n)&&n>=0&&n<=1)
  else if(kind==='Point'){const point=v.Vec2;valid=record(point)&&Object.keys(point).length===2&&['x','y'].every(k=>{const n=point[k];return n===null||(finite(n)&&Math.abs(n)<=3.4028234663852886e38)})}
  if(!valid)return `Invalid transition parameter ${id}`
 }
 return null
}
