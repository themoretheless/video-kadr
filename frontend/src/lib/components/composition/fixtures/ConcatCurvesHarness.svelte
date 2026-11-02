<script lang="ts">
 import Inspector from '../NestedConcatCurvesInspector.svelte'
 let token=$state('first')
 export function changeSession(){token='second'}
</script>
<Inspector {token}/>
