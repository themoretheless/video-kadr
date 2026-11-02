<script lang="ts">
 import Puck from '../ColorWheelPuck.svelte'
 let {oncommit}:{oncommit:(x:number,y:number,scope:unknown)=>void}=$props()
 let scope:object=$state.raw({revision:1})
 let clock=$state(0)
 export function seek(){clock+=1}
 let session=$state('first-session')
 let context=$state('root')
 export function logout(){session='second-session'}
 export function switchComposition(){context='other'}
 let disabled=$state(false)
 export function replaceScope(){scope={revision:2}}
 export function lock(){disabled=true}
</script>
<Puck x={0} y={0} label="Wheel" {scope} {clock} {session} {context} {disabled} {oncommit}/>
