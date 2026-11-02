<script lang="ts">
 import {onMount,tick} from 'svelte'
 import {observeResize} from '../../composables/resizeObserver'
 import {activityTransport} from './activityTransport.svelte'
 import {wireActivity,type ActivitySnapshot} from './activitySync'
 import {captureActivityViewport,restoreActivityViewport,initialActivityViewport,retainActivityExpansion,type ActivityViewport} from './activityViewport'
 import {activitySections,type ActivitySection} from './activityRows'
 import AgentActivityItem from './AgentActivityItem.svelte'
 let snapshot=$state.raw<ActivitySnapshot|null>(null)
 let filter=$state<'all'|'read'|'operation'|'error'>('all'),limit=$state(100)
 let expanded=$state<Record<string,boolean>>({}),following=$state(true),list:HTMLDivElement
 let viewport:ActivityViewport=initialActivityViewport(),layout=0,disposed=false
 const filtered=$derived((snapshot?.activities??[]).filter(item=>filter==='all'||filter==='error'&&item.state==='error'||filter==='read'&&item.kind==='read'||filter==='operation'&&item.kind!=='read'))
 const completedIds=$derived(new Set((snapshot?.completedIds??[]).filter(id=>filtered.some(item=>item.id===id)).slice(-limit)))
 const visible=$derived(filtered.filter(item=>item.state==='running'||completedIds.has(item.id)))
 const sections=$derived(activitySections(visible))
 function sessionLabel(id:string|null){return id===null?'Вне рабочей сессии':(snapshot?.sessions??[]).find(item=>item.id===id)?.reason??'Рабочая сессия'}
 function sessionOpen(section:ActivitySection){return section.sessionId===null||!(snapshot?.sessions??[]).find(item=>item.id===section.sessionId)?.endedAt||filter==='error'||section.rows.some(row=>row.kind==='activity'&&row.activity.state==='running')}
 function measurements(){
  const top=list.getBoundingClientRect().top
  return [...list.querySelectorAll<HTMLElement>('[data-agent-anchor]')].filter(row=>row.getClientRects().length>0).map(row=>{const rect=row.getBoundingClientRect();return {id:row.dataset.agentAnchor!,top:rect.top-top,bottom:rect.bottom-top}})
 }
 function capture(){if(list?.getClientRects().length){viewport=captureActivityViewport(list,measurements());following=viewport.following}}
 function restore(){const current=++layout;void tick().then(()=>{if(!disposed&&current===layout&&list?.getClientRects().length)list.scrollTop=restoreActivityViewport(viewport,list,measurements())})}
 function changeOpen(key:string,value:boolean){capture();expanded={...expanded,[key]:value};restore()}
 function latest(){viewport=initialActivityViewport();following=true;restore()}
 function showEarlier(){capture();viewport={...viewport,following:false};following=false;limit+=100;restore()}
 function changeFilter(event:Event){capture();filter=(event.currentTarget as HTMLSelectElement).value as typeof filter;restore()}
 onMount(()=>{
  const stop=wireActivity(activityTransport,value=>{
   capture()
   if(snapshot&&snapshot.workspaceId!==value.workspaceId){filter='all';limit=100;expanded={};viewport=initialActivityViewport();following=true}
   else if(snapshot&&snapshot.evicted!==value.evicted)expanded=retainActivityExpansion(expanded,value.activities.map(item=>item.id))
   snapshot=value;restore()
  })
  const stopResize=observeResize(list,()=>restore())
  return ()=>{disposed=true;layout++;stopResize();stop()}
 })
</script>
<section aria-label="Активность агента" data-activity-revision={snapshot?.revision} data-activity-workspace={snapshot?.workspaceId}>
 <label>Фильтр активности<select aria-label="Фильтр активности" value={filter} onchange={changeFilter}><option value="all">Все вызовы</option><option value="read">Чтение</option><option value="operation">Операции</option><option value="error">Ошибки</option></select></label>
 {#if !following}<button type="button" class="btn ghost sm" onclick={latest}>К последним вызовам</button>{/if}
 <!-- The scrollable log must support keyboard scrolling. -->
 <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
 <div class="activity-list" aria-label="Лента вызовов агента" role="log" aria-live="off" tabindex="0" bind:this={list} onscroll={capture}>
 {#if snapshot?.evicted}<p role="status">Удалено старых завершённых записей: {snapshot?.evicted}</p>{/if}
 {#each sections as section (section.id)}
  <details open={expanded[`session:${section.id}`]??sessionOpen(section)} data-agent-session={section.id}><summary data-agent-anchor={`session:${section.id}`} onclick={event=>{event.preventDefault();changeOpen(`session:${section.id}`,!(expanded[`session:${section.id}`]??sessionOpen(section)))}}>{sessionLabel(section.sessionId)}</summary>
   {#if section.sessionId&&(snapshot?.sessions??[]).find(item=>item.id===section.sessionId)?.endedAt}<p>Сессия завершена</p>{/if}
   <ol>
   {#each section.rows as row (row.id)}
    {#if row.kind==='activity'}<AgentActivityItem checkpoints={snapshot?.checkpoints??[]} historyLocked={snapshot?.lockReason!=null} workspaceId={snapshot?.workspaceId} item={row.activity} open={expanded[`call:${row.id}`]??false} onOpenChange={value=>changeOpen(`call:${row.id}`,value)} />
    {:else}<li><details open={expanded[`reads:${row.id}`]??false}><summary data-agent-anchor={`reads:${row.id}`} onclick={event=>{event.preventDefault();changeOpen(`reads:${row.id}`,!(expanded[`reads:${row.id}`]??false))}}>Быстрые чтения · {row.activities.length}</summary><ol>{#each row.activities as item (item.id)}<AgentActivityItem checkpoints={snapshot?.checkpoints??[]} historyLocked={snapshot?.lockReason!=null} workspaceId={snapshot?.workspaceId} {item} open={expanded[`call:${item.id}`]??false} onOpenChange={value=>changeOpen(`call:${item.id}`,value)} />{/each}</ol></details></li>{/if}
   {/each}
   </ol>
  </details>
 {/each}
 {#if filtered.length>visible.length}<button type="button" class="btn ghost sm" onclick={showEarlier}>Показать более ранние вызовы</button>{/if}
 {#if !filtered.length}<p>Вызовов пока нет</p>{/if}
 </div>
</section>
<style>.activity-list{max-height:28rem;overflow:auto;overflow-anchor:none;overscroll-behavior:contain}</style>
