<script lang="ts">
  import type { Snippet } from 'svelte'
  import {resolveEffectPreview}from'$lib/state/nestedEffectPreview.svelte'
  import { renderNestedPreview, type NativeNestedProject } from '$lib/api'

  interface Props {
    project: NativeNestedProject
    savedProject?: {projectId:string;expectedRevision:number}
    timeUs: number
    renderedOutputs: Readonly<Record<string, string>>
    token: string
    compositionId?: string
    overlay?: Snippet
    effectDraft?: boolean
    playing?: boolean
    playbackEpoch?: number
    label?: string
  }
  let { project, savedProject, timeUs, renderedOutputs, token, compositionId, overlay, effectDraft=false, playing=false, playbackEpoch=0, label="Предпросмотр вложенной композиции" }: Props = $props()
  let canvas: HTMLCanvasElement
  let loading = $state(false)
  let error = $state('')

  type Request={timeUs:number;project:NativeNestedProject;outputs:Readonly<Record<string,string>>;gesture?:object}
  let enqueue:((request:Request)=>void)|undefined=$state.raw()
  const noOutputs:Readonly<Record<string,string>>=Object.freeze({})
  $effect(()=>{
    const base=project,time=playing?null:timeUs,epoch=playbackEpoch,outputs=renderedOutputs,session=token,composition=compositionId,target=canvas
    const controller=new AbortController();let active=true,running=false,next:Request|undefined,latest:Request|undefined
    loading=true;error='';target?.getContext('2d')?.clearRect(0,0,target.width,target.height)
    async function pump(){
      if(running)return;running=true
      while(active&&next){const request=next;next=undefined;loading=true;error=''
        try{const frame=savedProject?await renderNestedPreview(request.project,request.timeUs,request.outputs,session,controller.signal,composition,savedProject):await renderNestedPreview(request.project,request.timeUs,request.outputs,session,controller.signal,composition)
          if(!active||!target||latest?.gesture!==request.gesture)continue
          target.width=frame.width;target.height=frame.height
          const context=target.getContext('2d');if(!context)throw new Error('Предпросмотр недоступен')
          const image=context.createImageData(frame.width,frame.height);image.data.set(frame.rgba);context.putImageData(image,0,0)
        }catch(reason){if(active&&!controller.signal.aborted&&latest?.gesture===request.gesture)error=reason instanceof Error?reason.message:'Не удалось загрузить кадр'}
      }
      running=false;if(active)loading=false
    }
    enqueue=request=>{if(!active)return;latest=request;next=request;void pump()}
    // Read semantic scope here; draft changes are queued separately below.
    void base;void outputs;void time;void epoch;void savedProject
    return()=>{active=false;next=undefined;controller.abort();enqueue=undefined}
  })
  $effect(()=>{
    const draft=effectDraft?resolveEffectPreview(project,timeUs,compositionId,token):null
    enqueue?.({timeUs,project:draft?.project??project,outputs:draft?noOutputs:renderedOutputs,gesture:draft?.gesture})
  })

</script>

<div class="nested-preview" aria-busy={loading}>
  <div class="native-preview-frame"><canvas bind:this={canvas} aria-label={label}></canvas>{@render overlay?.()}</div>
  {#if error}<p role="alert">{error}</p>{:else if loading}<p role="status">Загрузка кадра…</p>{/if}
</div>

<style>
  .nested-preview { position: relative; min-width: 0; }
  .native-preview-frame { position: relative; }
  canvas { display: block; width: 100%; height: auto; }
  p { margin: 0.5rem 0; font-size: 0.875rem; }
</style>
