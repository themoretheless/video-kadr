<script lang="ts">
 import Downloads from '../EnhancementModelDownloads.svelte'
 let {onready}:{onready:()=>void}=$props()
 let token=$state('owner')
</script>
<button onclick={()=>token='other'}>Сменить аккаунт</button>
<Downloads {token} {onready}/>
