<script lang="ts">
 import {untrack} from 'svelte'
 import Scopes from '../../src/lib/components/composition/NestedScopes.svelte'
 import type {NativeNestedProject} from '../../src/lib/api'
 let {project,token,timeUs,renderedOutputs,compositionId}:{project:NativeNestedProject;token:string;timeUs:number;renderedOutputs:Readonly<Record<string,string>>;compositionId:string}=$props()
 let cursor=$state(untrack(()=>timeUs))
</script>
<button onclick={()=>{cursor+=100000}}>Next scope frame</button>
<Scopes {project} {token} {renderedOutputs} {compositionId} timeUs={cursor} />
