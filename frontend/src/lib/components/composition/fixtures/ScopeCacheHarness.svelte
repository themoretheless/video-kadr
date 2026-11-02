<script lang="ts">
 import Scopes from '../NestedScopes.svelte'
 import type {NativeNestedProject} from '$lib/api'
 let {project}:{project:NativeNestedProject}=$props()
 let timeUs=$state(0),token=$state('first-session')
</script>
<button onclick={()=>{timeUs+=1000000}}>Advance scope cursor</button>
<button onclick={()=>{token='second-session'}}>Change scope session</button>
<Scopes {project} {timeUs} {token} renderedOutputs={{root:'movie'}} />
