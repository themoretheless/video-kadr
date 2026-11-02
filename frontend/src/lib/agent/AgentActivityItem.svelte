<script lang="ts">
 import ActivityCheckpointAction from './ActivityCheckpointAction.svelte'
 import {activityNavigationTarget,revealActivityAffected} from './activityNavigation.svelte'
 import {nestedHistory} from '../state/nestedComposition.svelte'
 import {SvelteDate} from 'svelte/reactivity'
 import type {AgentActivity} from './activity.svelte'
 let {item,open,onOpenChange,workspaceId,checkpoints=[],historyLocked=false}:{item:AgentActivity;checkpoints?:readonly {id:string;label:string}[];historyLocked?:boolean;workspaceId?:string;open?:boolean;onOpenChange?:(value:boolean)=>void}=$props()
 let localOpen=$state(false)
 const isOpen=$derived(open??localOpen)
 const navigation=$derived(activityNavigationTarget(item.affected??[],workspaceId))
 const entityNames={Layer:'Клип',Track:'Дорожка',Marker:'Маркер',Media:'Медиа',Transition:'Переход',Composition:'Композиция'}
 const labels:Record<string,string>={get_project:'Чтение проекта',get_history:'Чтение истории',list_checkpoints:'Список контрольных точек',create_checkpoint:'Создание контрольной точки',restore_checkpoint:'Восстановление контрольной точки',delete_checkpoint:'Удаление контрольной точки',merge_captions:'Объединение субтитров',restyle_captions:'Оформление субтитров',begin_agent_session:'Начало работы',end_agent_session:'Завершение работы',apply_batch:'Изменение проекта',dry_run:'Проверка изменений'}
</script>
<li data-agent-anchor={`call:${item.id}`} data-agent-activity={item.id} data-agent-checkpoint={item.checkpointId} data-activity-state={item.state}>
 <span>{labels[item.tool]??item.tool} · {item.state==='running'?'Выполняется':item.state==='error'?'Ошибка':'Завершено'}{item.durationMs===null?'':` · ${item.durationMs} мс`}</span>
 {#if item.checkpointLabel}<span>Контрольная точка: {item.checkpointLabel}</span>{/if}
 {#if item.kind==='checkpoint'&&item.checkpointId}<ActivityCheckpointAction id={item.checkpointId} {workspaceId} available={checkpoints.some(point=>point.id===item.checkpointId)} locked={historyLocked||item.state==='running'} />{/if}
 {#if item.historyIds.length}<span data-history-effect={item.effect}>{item.effect==='applied'?'Изменения применены':item.effect==='reverted'?'Изменения отменены':item.effect==='partial'?'Изменения применены частично':'Состояние изменений неизвестно'}</span>{/if}
 {#if item.affected?.length}<ul aria-label="Затронутые объекты">{#each item.affected as entity (JSON.stringify([entity.compositionId,entity.kind,entity.id]))}<li>{entity.label} · {entityNames[entity.kind]} · {entity.compositionId}</li>{/each}</ul>{/if}
 {#if item.affected?.some(entity=>entity.kind==='Layer'||entity.kind==='Track')}<button type="button" class="btn ghost sm" disabled={!navigation||navigation.mode==='nested'&&nestedHistory.busy} onclick={()=>revealActivityAffected(item.affected??[],workspaceId)}>{navigation?'Показать в редакторе':'Объекты недоступны'}</button>{/if}
 {#if item.affectedOmitted}<span>Ещё объектов: {item.affectedOmitted}</span>{/if}
 <details open={isOpen}><summary onclick={event=>{event.preventDefault();if(onOpenChange)onOpenChange(!isOpen);else localOpen=!isOpen}}>Подробности вызова</summary><p>{new SvelteDate(item.startedAt).toLocaleTimeString()} · {item.client??item.actor}{item.clientVersion?` (${item.clientVersion})`:``} · {item.actor} · {item.tool}</p><pre>{item.args}</pre>{#if item.error}<p role="alert">{item.error}</p>{/if}</details>
</li>
<style>pre{white-space:pre-wrap;overflow-wrap:anywhere}</style>
