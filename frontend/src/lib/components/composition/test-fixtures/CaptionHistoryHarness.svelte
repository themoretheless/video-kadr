<script lang="ts">
 import {untrack} from 'svelte'
 import Captions from '../CompositionCaptions.svelte'
 import type {Composition} from '$lib/composition/types'
 let {before,after}:{before:Composition;after:Composition}=$props()
 let composition=$state(untrack(()=>after))
</script>
<button onclick={()=>composition=before}>Restore before split</button>
<Captions {composition} onchange={next=>composition=next} />
