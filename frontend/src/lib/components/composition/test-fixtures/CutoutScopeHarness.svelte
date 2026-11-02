<script lang="ts">
 import Panel from '../ImageCutoutPanel.svelte'
 import type {MediaEntry} from '$lib/types'
 let{onaccepted}:{onaccepted:(entry:MediaEntry)=>void}=$props()
 let token=$state('owner'),context=$state<object>({version:1})
</script>
<button onclick={()=>token='other'}>Сменить аккаунт улучшения</button>
<button onclick={()=>context={version:2}}>Изменить проект при сохранении</button>
<Panel {token} {context} {onaccepted} projectId="project" sourceId="original" targetKey="clip" width={17} height={9}/>
