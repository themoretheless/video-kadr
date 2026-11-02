<script lang="ts">
 import {analyzeScopesBinary,renderNestedPreview,type NativeNestedPreviewFrame,type NativeNestedProject,type NativeScopeKind,type NativeScopeOptions,type NativeScopes} from '$lib/api'
 let {project,timeUs,renderedOutputs,token,compositionId}:{project:NativeNestedProject;timeUs:number;renderedOutputs:Readonly<Record<string,string>>;token:string;compositionId?:string}=$props()
 import {scopePresets} from '$lib/state/scopePresets'
 import ScopeGraph from './NativeScopeGraph.svelte'
 import {readScopePreferences,writeScopePreferences} from '$lib/state/scopePreferences'
 const preferences=readScopePreferences()
 let settingsOpen=$state(false),settingsX=$state(8),settingsY=$state(8)
 let settingsMenu=$state<HTMLDivElement|undefined>()
 let settingsReturn:HTMLElement|undefined
 function openSettings(target:HTMLElement,x:number,y:number){settingsReturn=target;settingsX=Math.max(8,Math.min(x,window.innerWidth-320));settingsY=Math.max(8,Math.min(y,window.innerHeight-200));settingsOpen=true}
 function closeSettings(){settingsOpen=false;settingsReturn?.focus()}
 $effect(()=>{if(settingsOpen&&settingsMenu)settingsMenu.querySelector<HTMLSelectElement>('select')?.focus()})
 let panelWidth=$state(600)
 let shown=$state<NativeScopeKind[]>(preferences.shown ?? [preferences.kind])
 let enabled=$state(false),kind=$state<NativeScopeKind>(preferences.kind),matrix=$state<NonNullable<NativeScopeOptions['matrix']>|'auto'>(preferences.matrix),clamp=$state(preferences.clamp)
 let waveformType=$state<NonNullable<NativeScopeOptions['waveformType']>>(preferences.waveformType),paradeType=$state<NonNullable<NativeScopeOptions['paradeType']>>(preferences.paradeType),brightness=$state<NonNullable<NativeScopeOptions['brightness']>>(preferences.brightness)
 let scale=$state<'bits8'|'float'|'hdr'>(preferences.scale)
 let targets=$state<'75'|'100'>(preferences.targets)
 let cachedPreview:{project:NativeNestedProject;time:number;outputs:Readonly<Record<string,string>>;session:string;composition:string|undefined;frame:NativeNestedPreviewFrame}|undefined
 let resultOwner:{project:NativeNestedProject;session:string;composition:string|undefined;settings:string}|undefined
 let loading=$state(false),error=$state(''),result=$state<NativeScopes|null>(null)
 $effect(()=>{writeScopePreferences({shown,kind,matrix,clamp,waveformType,paradeType,brightness,scale,targets})})
 const preset=$derived(scopePresets.find(p=>p.shown.length===shown.length&&p.shown.every((v,i)=>v===shown[i])&&p.waveformType===waveformType&&p.paradeType===paradeType&&p.scale===scale&&p.clamp===clamp)?.name ?? '')
 function applyPreset(name:string){const p=scopePresets.find(value=>value.name===name);if(!p)return;shown=[...p.shown];kind=shown[0]!;waveformType=p.waveformType;paradeType=p.paradeType;scale=p.scale;clamp=p.clamp}
 const names:Record<NativeScopeKind,string>={waveform:'Waveform',parade:'Parade',histogram:'Гистограмма',vectorscopeYuv:'Vectorscope YUV',vectorscopeHls:'Vectorscope HLS'}
 $effect(()=>{
  const show=enabled,document=project,time=timeUs,outputs=renderedOutputs,session=token,composition=compositionId
  const options:NativeScopeOptions={layoutWidth:Math.max(1,Math.min(16384,Math.round(panelWidth||600))),layoutHeight:440,scopes:shown,matrix:matrix==='auto'?undefined:matrix,colorSpace:matrix==='auto'?'auto':undefined,clamp,waveformType,paradeType,brightness,targets,scale}
  const requested=[...shown],abort=new AbortController();let active=true
  if(cachedPreview&&(cachedPreview.project!==document||cachedPreview.time!==time||cachedPreview.outputs!==outputs||cachedPreview.session!==session||cachedPreview.composition!==composition))cachedPreview=undefined
  const settings=JSON.stringify({requested,matrix,clamp,waveformType,paradeType,brightness,targets,scale})
  if(!show||!session||!resultOwner||resultOwner.project!==document||resultOwner.session!==session||resultOwner.composition!==composition||resultOwner.settings!==settings){result=null;resultOwner=undefined}
  error='';loading=show&&!!session
  if(show&&session)void (async()=>{
   try {
    const cached=cachedPreview
    const frame=cached&&cached.project===document&&cached.time===time&&cached.outputs===outputs&&cached.session===session&&cached.composition===composition
     ? cached.frame : await renderNestedPreview(document,time,outputs,session,abort.signal,composition)
    if(!active||abort.signal.aborted)return
    cachedPreview={project:document,time,outputs,session,composition,frame}
    const scopes=await analyzeScopesBinary(frame,options,abort.signal)
    if(!active||abort.signal.aborted)return
    if(requested.some(selected=>!scopes[selected]))throw new Error('Нет данных выбранного графика')
    resultOwner={project:document,session,composition,settings}
    result=scopes
   }catch(failure){if(active&&!abort.signal.aborted)error=failure instanceof Error?failure.message:String(failure)}finally{if(active)loading=false}
  })()
  return ()=>{active=false;abort.abort()}
 })
</script>
 {#snippet settingsControls()}
 <div class="controls">
 <label>Пресет<select aria-label="Scopes пресет" value={preset} onchange={e=>{applyPreset(e.currentTarget.value)}}><option value="">Пользовательский</option>{#each scopePresets as p}<option value={p.name}>{p.name}</option>{/each}</select></label>
 <label>График<select aria-label="Scopes график" value={kind} onchange={e=>{kind=e.currentTarget.value as NativeScopeKind;shown=[kind]}}>{#each Object.entries(names) as [id,name]}<option value={id}>{name}</option>{/each}</select></label>
 <div class="scope-toggles">{#each Object.entries(names) as [id,name]}<label><input type="checkbox" aria-label={`Scopes показать ${name}`} checked={shown.includes(id as NativeScopeKind)} disabled={shown.length===1&&shown[0]===id} onchange={e=>{shown=e.currentTarget.checked?[...shown,id as NativeScopeKind]:shown.filter(value=>value!==id)}} />{name}</label>{/each}</div>
 <label>Матрица<select aria-label="Scopes матрица" value={matrix} onchange={e=>{matrix=e.currentTarget.value as NonNullable<NativeScopeOptions['matrix']>|'auto'}}><option value="auto">Автоматически (SDR)</option><option value="Bt601">BT.601</option><option value="Bt709">BT.709</option><option value="Bt2020Ncl">BT.2020 NCL</option></select></label>
 <label>Яркость<select aria-label="Scopes яркость" value={brightness} onchange={e=>{brightness=e.currentTarget.value as NonNullable<NativeScopeOptions['brightness']>}}><option value="dimmed">Пониженная</option><option value="normal">Обычная</option><option value="bright">Повышенная</option></select></label>
 {#if shown.includes('waveform')}<label>Режим<select aria-label="Scopes waveform режим" value={waveformType} onchange={e=>{waveformType=e.currentTarget.value as NonNullable<NativeScopeOptions['waveformType']>}}><option value="rgb">RGB</option><option value="luma">Luma</option><option value="yc">YC</option><option value="ycNoChroma">YC без цветности</option></select></label>{/if}
 {#if shown.includes('parade')}<label>Режим<select aria-label="Scopes parade режим" value={paradeType} onchange={e=>{paradeType=e.currentTarget.value as NonNullable<NativeScopeOptions['paradeType']>}}><option value="rgb">RGB</option><option value="yuv">YUV</option><option value="rgbWhite">RGB + белый</option></select></label>{/if}
 {#if shown.includes('vectorscopeYuv')}<label>Мишени<select aria-label="Scopes мишени" value={targets} onchange={e=>{targets=e.currentTarget.value as '75'|'100'}}><option value="75">75 %</option><option value="100">100 %</option></select></label>{/if}
 <label>Шкала<select aria-label="Scopes шкала" value={scale} onchange={e=>{scale=e.currentTarget.value as 'bits8'|'float'|'hdr'}}><option value="bits8">8 Bit</option><option value="float">Float</option><option value="hdr">HDR (SDR 100 кд/м²)</option></select></label>
 <label><input type="checkbox" aria-label="Scopes ограничить сигнал" checked={clamp} onchange={e=>{clamp=e.currentTarget.checked}} /> Ограничить сигнал</label>
 </div>
 {/snippet}
<details class="scopes" ontoggle={e=>{enabled=e.currentTarget.open}}>
 <summary>Lumetri Scopes</summary>

 <button type="button" data-scope-settings aria-label="Настройки Scopes" aria-expanded={settingsOpen} onclick={e=>{if(settingsOpen)closeSettings();else{const r=e.currentTarget.getBoundingClientRect();openSettings(e.currentTarget,r.left,r.bottom)}}}>Настройки</button>
 {@render settingsControls()}
 <div role="button" aria-haspopup="dialog" aria-label="Настройки графиков Scopes" onclick={e=>{openSettings(e.currentTarget,e.clientX,e.clientY)}} tabindex="0" oncontextmenu={e=>{e.preventDefault();openSettings(e.currentTarget,e.clientX,e.clientY)}} onkeydown={e=>{if(e.key==='Enter'||e.key===' '||e.key==='ContextMenu'||(e.shiftKey&&e.key==='F10')){e.preventDefault();const r=e.currentTarget.getBoundingClientRect();openSettings(e.currentTarget,r.left,r.top)}}} aria-busy={loading} class="graphs" bind:clientWidth={panelWidth} style:grid-template-columns={`repeat(${result?.layout?.columns ?? 1},minmax(0,1fr))`}>{#each shown as selected (selected)}<section aria-label={names[selected]}><div class="scope-name">{result?.[selected]?.title ?? names[selected]}</div><ScopeGraph graph={result?.[selected]} height={shown.length>1 ? Math.max(80,440/(result?.layout?.rows ?? shown.length)-24) : 220} /></section>{/each}</div>
 {#if result?.resolvedColorSpace}<div class="space-label" aria-label="Цветовое пространство Scopes">{{rec601:'Rec. 601',rec709:'Rec. 709',rec2100:'Rec. 2100'}[result.resolvedColorSpace]}</div>{/if}
 {#if !token}<p role="status">Войдите для анализа предпросмотра</p>{:else if error}<p role="alert">{error}</p>{:else if loading}<p role="status">Расчёт Scopes…</p>{/if}
 {#if result?.stats}
 <table aria-label="Scopes уровни"><thead><tr><th>Канал</th><th>Мин., %</th><th>Макс., %</th><th>Среднее, %</th></tr></thead><tbody>{#each ['r','g','b','y'] as channel}{@const level=result.stats[channel as 'r'|'g'|'b'|'y']}<tr><th>{channel.toUpperCase()}</th><td>{level.min.toFixed(2)}</td><td>{level.max.toFixed(2)}</td><td>{level.mean.toFixed(2)}</td></tr>{/each}</tbody></table>
 {/if}
</details>
{#if settingsOpen}<div class="settings-popup" role="dialog" aria-label="Настройки Scopes" bind:this={settingsMenu} style:left={`${settingsX}px`} style:top={`${settingsY}px`}><button type="button" onclick={closeSettings}>Закрыть настройки</button>{@render settingsControls()}</div>{/if}
<svelte:window onkeydown={e=>{if(settingsOpen&&e.key==='Escape'){e.preventDefault();closeSettings()}}} onpointerdown={e=>{if(settingsOpen&&settingsMenu&&!e.composedPath().includes(settingsMenu)&&!(e.target instanceof Element&&e.target.closest('[data-scope-settings]')))closeSettings()}} />
<style>
 .settings-popup{position:fixed;z-index:1000;width:min(500px,calc(100vw - 16px));max-height:calc(100vh - 24px);overflow:auto;padding:8px;background:var(--color-surface,#222);border:1px solid #555;border-radius:4px;color:inherit}
 .graphs{display:grid;gap:8px}.graphs section{min-width:0}.scope-name,.space-label{font-size:11px}.space-label{padding:4px 0;color:#aaa}.scope-toggles{display:flex;flex-wrap:wrap;gap:8px;width:100%}
.scopes{min-width:0}.controls{display:flex;flex-wrap:wrap;gap:8px;align-items:center;padding:8px 0}label{display:flex;gap:4px;align-items:center;font-size:12px}table{font-size:12px;width:100%;text-align:right;border-collapse:collapse}th,td{padding:3px 6px}th:first-child{text-align:left}
</style>
