<script setup lang="ts">
import { cancelQueuedExport, exportQueueState, retryQueuedExport } from '../store'

const labels: Record<string, string> = {
  queued: 'В очереди', running: 'Выполняется', done: 'Готово', error: 'Ошибка',
  cancelled: 'Отменено', interrupted: 'Прервано', permission_required: 'Нужно разрешение',
}
const retryable = (status: string) => ['error', 'interrupted', 'cancelled', 'permission_required'].includes(status)
const cancellable = (status: string) => ['queued', 'running', 'permission_required'].includes(status)
const lutPermission = (task: { recoveryKind?: 'source' | 'lut' }) => task.recoveryKind === 'lut'
</script>

<template>
  <section v-if="exportQueueState.restoring || exportQueueState.tasks.length" class="export-queue" aria-labelledby="export-queue-title">
    <header>
      <div><h2 id="export-queue-title">Очередь экспорта</h2><p>{{ exportQueueState.message }}</p></div>
      <span v-if="exportQueueState.restoring" role="status">Восстановление очереди…</span>
    </header>
    <p class="hint">Определения и история очереди сохраняются после перезагрузки. Ссылка на готовый browser-файл действует только в текущей сессии. WASM-encode не имеет checkpoint: прерванная попытка автоматически запускается заново с 0% в новом движке.</p>
    <ol>
      <li v-for="task in exportQueueState.tasks" :key="task.id">
        <div>
          <strong>{{ task.name }}</strong>
          <span class="task-state">{{ labels[task.status] ?? task.status }}</span>
          <span v-if="task.attempt > 1" class="restart-badge">Перезапуск с 0% · попытка {{ task.attempt }} · рестартов: {{ task.restartCount }}</span>
          <span v-if="task.stage">{{ task.stage }}</span>
          <progress v-if="task.status === 'running' && typeof task.progress === 'number'" :value="task.progress" max="100" :aria-label="`Прогресс ${task.name}: ${task.progress}%`">{{ task.progress }}%</progress>
          <small v-if="task.error" class="error">{{ task.error }}</small>
          <small v-if="task.status === 'permission_required' && lutPermission(task)">Выберите LUT заново в библиотеке или повторно загрузите файл .cube, затем повторите вариант.</small>
          <small v-else-if="task.status === 'permission_required'">Перепривяжите исходник в медиатеке, затем повторите вариант.</small>
        </div>
        <div class="export-queue-actions">
          <a v-if="task.status === 'done' && task.result?.url" class="btn primary sm" :href="task.result.url" :download="task.result.filename" :aria-label="`Скачать ${task.name}; ссылка текущей сессии`">Скачать {{ task.name }}</a>
          <a v-if="task.status === 'permission_required' && lutPermission(task)" class="btn ghost sm" href="#lut-library" :aria-label="`Перейти в библиотеку LUT для восстановления ${task.name}`">Открыть библиотеку LUT</a>
          <a v-else-if="task.status === 'permission_required'" class="btn ghost sm" href="#media-library" :aria-label="`Перейти в медиатеку для перепривязки ${task.name}`">Перепривязать источник</a>
          <button v-if="cancellable(task.status)" type="button" class="btn ghost sm" :aria-label="`Отменить ${task.name}`" @click="cancelQueuedExport(task.id)">Отменить</button>
          <button v-if="retryable(task.status)" type="button" class="btn primary sm" :aria-label="`${task.status === 'permission_required' ? 'Повторить после перепривязки' : 'Начать заново'} ${task.name}`" @click="retryQueuedExport(task.id)">{{ task.status === 'permission_required' ? 'Повторить после перепривязки' : 'Начать заново' }}</button>
        </div>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.export-queue { display:grid; gap:.7rem; margin-top:1rem; padding:.8rem; border:1px solid var(--border); border-radius:.7rem; }
.export-queue header,.export-queue li,.export-queue-actions { display:flex; justify-content:space-between; align-items:center; gap:.7rem; }
.export-queue h2,.export-queue p { margin:0; }
.export-queue ol { display:grid; gap:.55rem; margin:0; padding-left:1.5rem; }
.export-queue li { padding:.55rem; border:1px solid var(--border); border-radius:.55rem; }
.export-queue li > div:first-child { display:grid; gap:.25rem; min-width:0; }
.task-state { color:var(--muted); }
.restart-badge { color:var(--accent); font-size:.8rem; font-weight:700; }
progress { width:min(18rem,100%); }
@media (max-width:560px) { .export-queue header,.export-queue li { align-items:stretch; flex-direction:column; } }
</style>
