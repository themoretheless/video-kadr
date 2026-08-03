<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  cancelRecording,
  pauseRecording,
  recordingCapabilities,
  recordingState,
  retrySavingRecording,
  resumeRecording,
  startRecording,
  stopRecording,
} from '../recording-store'

const screen = ref(true)
const camera = ref(false)
const microphone = ref(true)
const systemAudio = ref(true)
const cameraCorner = ref<'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'>('bottom-right')
const cameraScale = ref(0.3)
const fps = ref(30)
const preview = ref<HTMLVideoElement | null>(null)
const confirmDiscard = ref(false)

const active = computed(() => ['requesting', 'recording', 'paused', 'stopping', 'saving'].includes(recordingState.phase))
const canStart = computed(() => recordingCapabilities.supported
  && (screen.value || camera.value)
  && (!screen.value || recordingCapabilities.screen)
  && (!camera.value || recordingCapabilities.camera)
  && !recordingState.hasPendingCapture
  && !active.value)
const elapsed = computed(() => {
  const seconds = Math.max(0, Math.floor(recordingState.elapsedSeconds))
  return `${Math.floor(seconds / 60).toString().padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`
})
const previewSummary = computed(() => {
  const sources = [screen.value && 'экран', camera.value && 'камера', microphone.value && 'микрофон'].filter(Boolean).join(' + ')
  return camera.value && screen.value ? `${sources}; камера ${cameraCorner.value}, ${Math.round(cameraScale.value * 100)}%` : sources
})

watch(() => recordingState.previewStream, async stream => {
  await nextTick()
  if (!preview.value) return
  preview.value.srcObject = stream
  if (stream) void preview.value.play().catch(() => undefined)
}, { immediate: true })

onUnmounted(() => {
  window.removeEventListener('pagehide', onPageHide)
  if (['requesting', 'recording', 'paused'].includes(recordingState.phase)) void cancelRecording()
})

function onPageHide(): void {
  if (['requesting', 'recording', 'paused'].includes(recordingState.phase)) void cancelRecording()
}

onMounted(() => window.addEventListener('pagehide', onPageHide))

function begin(): void {
  confirmDiscard.value = false
  if (!canStart.value) return
  void startRecording({
    screen: screen.value, camera: camera.value, microphone: microphone.value,
    systemAudio: screen.value && systemAudio.value,
    cameraCorner: cameraCorner.value, cameraScale: cameraScale.value, fps: fps.value,
  })
}

function requestDiscard(): void {
  if (!active.value && !recordingState.hasPendingCapture) { void cancelRecording(); return }
  confirmDiscard.value = true
}

function discard(): void {
  confirmDiscard.value = false
  void cancelRecording()
}
</script>

<template>
  <section class="recorder-panel" aria-labelledby="recorder-title">
    <header>
      <div>
        <h2 id="recorder-title">Запись экрана и камеры</h2>
        <p>Запись выполняется локально. Браузер запросит доступ только после нажатия «Начать запись».</p>
      </div>
      <span v-if="active" class="recording-indicator" role="status" aria-live="polite">● {{ recordingState.phase === 'paused' ? 'Запись приостановлена' : 'Идёт захват' }} · <time>{{ elapsed }}</time></span>
    </header>

    <p v-if="!recordingCapabilities.supported" class="error" role="alert">{{ recordingCapabilities.reason || 'Запись не поддерживается этим браузером.' }}</p>

    <fieldset :disabled="active">
      <legend>Источники записи</legend>
      <label><input v-model="screen" type="checkbox" :disabled="!recordingCapabilities.screen"> Экран</label>
      <small v-if="!recordingCapabilities.screen">Захват экрана недоступен.</small>
      <label><input v-model="camera" type="checkbox" :disabled="!recordingCapabilities.camera"> Камера</label>
      <small v-if="!recordingCapabilities.camera">Камера недоступна.</small>
      <label><input v-model="microphone" type="checkbox"> Микрофон</label>
      <label v-if="screen"><input v-model="systemAudio" type="checkbox"> Звук вкладки или системы, если браузер его предоставит</label>
    </fieldset>

    <fieldset v-if="camera" :disabled="active">
      <legend>Камера в кадре</legend>
      <label>Положение
        <select v-model="cameraCorner">
          <option value="top-left">Слева сверху</option><option value="top-right">Справа сверху</option>
          <option value="bottom-left">Слева снизу</option><option value="bottom-right">Справа снизу</option>
        </select>
      </label>
      <label>Размер
        <select v-model.number="cameraScale"><option :value="0.2">20%</option><option :value="0.3">30%</option><option :value="0.4">40%</option></select>
      </label>
    </fieldset>

    <label class="fps-setting">Частота записи
      <select v-model.number="fps" :disabled="active"><option :value="15">15 fps</option><option :value="30">30 fps</option></select>
    </label>

    <div v-if="recordingState.previewStream" class="recording-preview">
      <video ref="preview" muted autoplay playsinline aria-label="Предпросмотр записи"></video>
      <p>{{ previewSummary }}</p>
    </div>
    <p v-else class="hint">Композиция: {{ previewSummary || 'выберите экран или камеру' }}.</p>

    <div v-if="recordingState.warnings.length" class="recording-warnings" role="status" aria-live="polite">
      <p v-for="warning in recordingState.warnings" :key="warning">{{ warning }}</p>
    </div>
    <p v-if="recordingState.error" class="error" role="alert">{{ recordingState.error }}</p>
    <button v-if="recordingState.canRetrySave" type="button" class="btn primary" @click="retrySavingRecording">Повторить сохранение</button>
    <button v-if="recordingState.canRetrySave" type="button" class="btn danger" @click="requestDiscard">Удалить несохранённую запись</button>
    <p v-if="!screen && !camera" class="error" role="alert">Выберите экран или камеру.</p>
    <p v-if="recordingState.phase === 'done'" role="status">Запись «{{ recordingState.savedName }}» сохранена в медиатеке и добавлена на timeline.</p>

    <div class="recorder-actions">
      <button v-if="!active" type="button" class="btn primary" :disabled="!canStart" @click="begin">Начать запись</button>
      <button v-if="recordingState.phase === 'recording'" type="button" class="btn ghost" :aria-pressed="false" @click="pauseRecording">Пауза</button>
      <button v-if="recordingState.phase === 'paused'" type="button" class="btn ghost" :aria-pressed="true" @click="resumeRecording">Продолжить</button>
      <button v-if="recordingState.phase === 'recording' || recordingState.phase === 'paused'" type="button" class="btn primary" @click="stopRecording">Завершить и сохранить</button>
      <button v-if="recordingState.phase === 'requesting' || recordingState.phase === 'recording' || recordingState.phase === 'paused'" type="button" class="btn ghost" @click="requestDiscard">Отменить запись</button>
      <span v-if="recordingState.phase === 'requesting'" role="status">Запрашиваю выбранные разрешения…</span>
      <span v-if="recordingState.phase === 'stopping' || recordingState.phase === 'saving'" role="status">Завершаю и сохраняю запись…</span>
    </div>

    <div v-if="confirmDiscard" class="discard-confirmation" role="alertdialog" aria-labelledby="discard-title" aria-describedby="discard-description">
      <strong id="discard-title">Удалить текущую запись?</strong>
      <p id="discard-description">Несохранённые данные записи будут потеряны.</p>
      <button type="button" class="btn ghost" @click="confirmDiscard = false">Продолжить запись</button>
      <button type="button" class="btn danger" @click="discard">Удалить запись</button>
    </div>
  </section>
</template>

<style scoped>
.recorder-panel { display:grid; gap:.75rem; margin:1rem 0; padding:1rem; border:1px solid var(--border); border-radius:.8rem; }
.recorder-panel header,.recorder-actions { display:flex; justify-content:space-between; align-items:center; gap:.75rem; flex-wrap:wrap; }
.recorder-panel h2,.recorder-panel p { margin:0; }
.recorder-panel fieldset { display:flex; gap:.75rem; flex-wrap:wrap; border:1px solid var(--border); border-radius:.55rem; }
.recording-indicator { color:var(--danger); font-weight:700; }
.recording-preview video { width:min(100%,42rem); max-height:24rem; background:#000; border-radius:.55rem; }
.discard-confirmation { display:grid; gap:.5rem; padding:.75rem; border:1px solid var(--danger); border-radius:.55rem; }
</style>
