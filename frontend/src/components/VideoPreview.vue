<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { beginEditTransaction, clientOnlyMode, endEditTransaction, isIdentityCurves, setProjectProxyPolicy, state, timelineState, type ProjectProxyPolicy } from '../store'
import { derivedTaskState, regenerateMissingBrowserProxy } from '../derived-task-center'
import { browserProxyCapability, deleteBrowserProxyArtifact, resolveBrowserPreviewSource, type ProxyPreviewSource } from '../browser-proxy-artifacts'
import { invalidateBackendProxyArtifact, resolveBackendPreviewSource } from '../proxy-preview'
import RectOverlay from './RectOverlay.vue'

const videoEl = ref<HTMLVideoElement | null>(null)
const previewSource = ref<ProxyPreviewSource>({ url: '', usingProxy: false, status: 'original' })
const proxyError = ref('')
let previewGeneration = 0
let pendingSwitch: { time: number; playing: boolean; revoke?: () => void } | null = null
let pendingRestoreCleanup: (() => void) | null = null

const proxyPolicy = computed<ProjectProxyPolicy>(() => timelineState.document?.proxyPolicy ?? 'auto')
const proxyCapability = computed(() => {
  if (!clientOnlyMode) return true
  return browserProxyCapability().supported
})

function releasePreview(source = previewSource.value): void { source.revoke?.() }

async function resolvePreview(): Promise<void> {
  const video = state.video
  const generation = ++previewGeneration
  const original = video?.url ?? ''
  if (!video) {
    releasePreview(); previewSource.value = { url: '', usingProxy: false, status: 'original' }; return
  }
  let next: ProxyPreviewSource = { url: original, usingProxy: false, status: 'missing' }
  proxyError.value = ''
  try {
    if (clientOnlyMode) {
      next = await resolveBrowserPreviewSource(video, proxyPolicy.value, proxyCapability.value)
      if (next.status === 'missing' && video.fingerprint) void regenerateMissingBrowserProxy(video.fingerprint)
    } else {
      next = await resolveBackendPreviewSource(video, proxyPolicy.value, derivedTaskState.tasks)
    }
  } catch (error) {
    proxyError.value = error instanceof Error ? error.message : String(error)
  }
  if (generation !== previewGeneration || state.video?.id !== video.id) { next.revoke?.(); return }
  const previous = previewSource.value
  const el = videoEl.value
  pendingSwitch = { time: el?.currentTime ?? state.playerTime, playing: Boolean(el && !el.paused), revoke: previous.revoke }
  previewSource.value = next
  if (previous.url === next.url) { pendingSwitch = null; previous.revoke?.() }
}

function changeProxyPolicy(event: Event): void {
  setProjectProxyPolicy((event.target as HTMLSelectElement).value as ProjectProxyPolicy)
}

// While playing, loop within the trim region so the preview reflects the cut.
function onTimeUpdate() {
  const el = videoEl.value
  if (!el) return
  state.playerTime = el.currentTime
  applyPlayback(el)
  if (el.paused) return
  const { trimStart, trimEnd } = state.edit
  if (el.currentTime > trimEnd) {
    el.currentTime = trimStart
  }
}

// Live preview of speed/volume (color + flip are pure CSS via videoStyle).
function applyPlayback(el: HTMLVideoElement) {
  const s = state.edit.speed
  if (s > 0 && el.playbackRate !== s) el.playbackRate = s
  const v = Math.min(1, Math.max(0, state.edit.volume))
  if (el.volume !== v) el.volume = v
}

watch(
  () => [state.edit.speed, state.edit.volume],
  () => {
    if (videoEl.value) applyPlayback(videoEl.value)
  },
)

// CSS approximation of the colour/flip effects so the user sees them live.
const videoStyle = computed(() => {
  const e = state.edit
  const f: string[] = []
  if (e.brightness) f.push(`brightness(${(1 + e.brightness).toFixed(3)})`)
  if (e.contrast !== 1) f.push(`contrast(${e.contrast})`)
  if (e.saturation !== 1) f.push(`saturate(${e.saturation})`)
  const presets: Record<string, string> = {
    grayscale: 'grayscale(1)',
    sepia: 'sepia(0.6)',
    warm: 'sepia(0.4) saturate(1.3)',
    cold: 'hue-rotate(-12deg) saturate(1.15)',
    'teal-orange': 'contrast(1.1) saturate(1.2) hue-rotate(-6deg)',
    faded: 'contrast(0.85) brightness(1.05) saturate(0.9)',
    noir: 'grayscale(1) contrast(1.4)',
    vintage: 'sepia(0.3) contrast(0.95) saturate(1.1)',
  }
  if (e.filter && presets[e.filter]) f.push(presets[e.filter])
  const sx = e.flipH ? -1 : 1
  const sy = e.flipV ? -1 : 1
  return {
    filter: f.length ? f.join(' ') : undefined,
    transform: sx !== 1 || sy !== 1 ? `scaleX(${sx}) scaleY(${sy})` : undefined,
  }
})

const advancedColorNotice = computed(() => {
  const lutActive = Boolean(state.edit.lutId) && state.edit.lutIntensity > 0
  const curvesActive = !isIdentityCurves(state.edit.curves)
  if (lutActive && curvesActive) return 'LUT и кривые включены.'
  if (lutActive) return 'LUT включён.'
  if (curvesActive) return 'Кривые включены.'
  return ''
})

// Reload the player when a new source is imported.
watch(
  () => [state.video?.id, state.video?.url, state.video?.fingerprint, proxyPolicy.value,
    derivedTaskState.tasks.map(task => `${task.id}:${task.state}:${task.idempotencyKey}`).join('|')],
  () => void resolvePreview(), { immediate: true },
)

watch(() => previewSource.value.url, () => {
  const el = videoEl.value
  if (!el) return
  pendingRestoreCleanup?.()
  const switching = pendingSwitch ?? { time: el.currentTime, playing: !el.paused }
  pendingSwitch = null
  el.load()
  let released = false
  const release = () => { if (!released) { released = true; switching.revoke?.() } }
  const restore = () => {
    el.currentTime = Math.min(switching.time, Number.isFinite(el.duration) ? el.duration : switching.time)
    applyPlayback(el)
    if (switching.playing) void el.play().catch(() => undefined)
    release()
    pendingRestoreCleanup = null
  }
  const cleanup = () => {
    el.removeEventListener('loadedmetadata', restore)
    release()
    if (pendingRestoreCleanup === cleanup) pendingRestoreCleanup = null
  }
  pendingRestoreCleanup = cleanup
  el.addEventListener('loadedmetadata', restore, { once: true })
})

function onPreviewError(): void {
  if (!previewSource.value.usingProxy || !state.video) return
  const previous = previewSource.value
  previewSource.value = { url: state.video.url, usingProxy: false, status: 'stale' }
  previous.revoke?.()
  proxyError.value = 'Proxy недоступен; предпросмотр продолжен с оригинала.'
  if (clientOnlyMode && state.video.fingerprint) {
    const failedFingerprint = state.video.fingerprint
    void deleteBrowserProxyArtifact(failedFingerprint)
      .catch(() => undefined)
      .finally(() => regenerateMissingBrowserProxy(failedFingerprint))
  } else if (previous.artifactKey) {
    void invalidateBackendProxyArtifact(previous.artifactKey).catch(() => undefined)
  }
}

onBeforeUnmount(() => {
  previewGeneration++
  pendingRestoreCleanup?.()
  pendingSwitch?.revoke?.()
  pendingSwitch = null
  releasePreview()
})

// Player bridge: react to seek requests and play/pause toggles from hotkeys.
watch(
  () => state.seekTo,
  (t) => {
    if (t != null && videoEl.value) {
      videoEl.value.currentTime = t
      state.seekTo = null
    }
  },
)

watch(
  () => state.playToggle,
  () => {
    const el = videoEl.value
    if (!el) return
    if (el.paused) void el.play()
    else el.pause()
  },
)

function fmtDuration(t: number): string {
  if (!isFinite(t)) return '0:00'
  const m = Math.floor(t / 60)
  const s = Math.floor(t % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

function fmtSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} ГБ`
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} МБ`
  return `${Math.max(1, Math.round(bytes / 1024))} КБ`
}

const meta = computed(() => {
  const v = state.video
  if (!v) return [] as string[]
  const parts: string[] = []
  if (v.width && v.height) parts.push(`${v.width}×${v.height}`)
  parts.push(`${fmtDuration(v.duration)}`)
  if (v.fps) parts.push(`${v.fps.toFixed(1)} fps`)
  const codecs = [v.vcodec, v.acodec].filter(Boolean).join(' / ')
  if (codecs) parts.push(codecs)
  if (typeof v.sizeBytes === 'number') parts.push(fmtSize(v.sizeBytes))
  return parts
})
</script>

<template>
  <div class="card preview">
    <h2 v-if="state.video?.title" class="preview-title" :title="state.video.title">
      {{ state.video.title }}
    </h2>
    <div class="player-wrap">
      <video
        ref="videoEl"
        class="player"
        :src="previewSource.url || state.video?.url"
        :style="videoStyle"
        :muted="state.edit.mute"
        controls
        playsinline
        @timeupdate="onTimeUpdate"
        @error="onPreviewError"
      ></video>
      <RectOverlay
        v-if="state.video && state.edit.cropEnabled"
        :rect="state.edit.crop"
        @update:rect="state.edit.crop = $event"
        @interaction-start="beginEditTransaction('crop-drag')"
        @interaction-end="endEditTransaction"
      />
      <RectOverlay
        v-if="state.video && state.edit.censorEnabled"
        :rect="state.edit.censor"
        color="var(--danger)"
        mode="mask"
        @update:rect="state.edit.censor = $event"
        @interaction-start="beginEditTransaction('censor-drag')"
        @interaction-end="endEditTransaction"
      />
    </div>
    <div v-if="state.video" class="meta">
      <span v-for="(m, i) in meta" :key="i" class="meta-chip">{{ m }}</span>
    </div>
    <div v-if="state.video" class="proxy-controls">
      <label for="proxy-policy">Источник предпросмотра</label>
      <select id="proxy-policy" :value="proxyPolicy" @change="changeProxyPolicy">
        <option value="auto">Авто</option>
        <option value="proxy">Proxy</option>
        <option value="original">Оригинал</option>
      </select>
      <span class="meta-chip" role="status">
        {{ previewSource.usingProxy ? 'Proxy' : previewSource.status === 'unsupported' ? 'Proxy не поддерживается' : previewSource.status === 'stale' ? 'Proxy устарел — оригинал' : previewSource.status === 'missing' ? 'Proxy готовится — оригинал' : 'Оригинал' }}
      </span>
    </div>
    <p v-if="proxyError" class="hint" role="status">{{ proxyError }}</p>
    <p v-if="previewSource.usingProxy && previewSource.hasAudio === false" class="hint" role="status">
      Этот browser proxy без аудиодорожки; выберите «Оригинал» для контроля звука.
    </p>
    <p v-if="advancedColorNotice" class="preview-color-notice" role="status">
      <strong>{{ advancedColorNotice }}</strong> Эти настройки не отображаются в предпросмотре;
      точный результат виден после экспорта.
    </p>
    <p v-if="state.video" class="hint">Обрезка зациклена внутри выбранного отрезка. Экспорт всегда читает оригинал.</p>
  </div>
</template>
