<script lang="ts">
 import Inspector from'../NestedConcatAdjustInspector.svelte'
 let token=$state('first')
 export function changeSession(){token='second'}
</script>
<Inspector {token}/>
