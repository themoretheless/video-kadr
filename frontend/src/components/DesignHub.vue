<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
import {
  createBrandKit,
  deleteBrandKit,
  deleteTemplate,
  designState,
  instantiateTemplate,
  loadDesignCatalog,
  saveCurrentProjectTemplate,
} from '../design-store'

const props = defineProps<{ hasActiveProject: boolean }>()
type Tab = 'templates' | 'kits'
type PlaceholderKind = 'video' | 'audio' | 'text' | 'logo'
interface PlaceholderDraft { key: string; label: string; kind: PlaceholderKind; required: boolean }

const tab = ref<Tab>('templates')
const templateName = ref('')
const placeholders = ref<PlaceholderDraft[]>([])
const templateKitId = ref('')
const kitName = ref('')
const colors = ref([{ key: crypto.randomUUID(), name: 'Основной', value: '#2463eb' }])
const fontFiles = ref<File[]>([])
const logoFiles = ref<File[]>([])
const confirmDelete = ref<{ type: 'template' | 'kit'; id: string; name: string } | null>(null)
const confirmApply = ref<{ id: string; name: string } | null>(null)
const confirmation = ref<HTMLElement | null>(null)
let confirmationInvoker: HTMLElement | null = null
const localError = ref('')

onMounted(() => { void loadDesignCatalog() })

function addPlaceholder(): void {
  placeholders.value.push({ key: crypto.randomUUID(), label: '', kind: 'video', required: true })
}
function addColor(): void { colors.value.push({ key: crypto.randomUUID(), name: '', value: '#000000' }) }
const removePlaceholder = (key: string) => { placeholders.value = placeholders.value.filter(item => item.key !== key) }
const removeColor = (key: string) => { colors.value = colors.value.filter(item => item.key !== key) }
const files = (event: Event) => [...((event.target as HTMLInputElement).files ?? [])]

async function saveTemplate(): Promise<void> {
  localError.value = ''
  const name = templateName.value.trim()
  if (!name) { localError.value = 'Введите название шаблона.'; return }
  if (placeholders.value.some(item => !item.label.trim())) { localError.value = 'Назовите каждый placeholder.'; return }
  const normalized = placeholders.value.map(item => item.label.trim().toLocaleLowerCase())
  if (new Set(normalized).size !== normalized.length) { localError.value = 'Названия placeholder должны отличаться.'; return }
  const drafts = placeholders.value.map(item => ({ label: item.label.trim(), kind: item.kind, required: item.required }))
  if (templateKitId.value) await saveCurrentProjectTemplate(name, drafts, templateKitId.value)
  else await saveCurrentProjectTemplate(name, drafts)
  if (!designState.error) { templateName.value = ''; placeholders.value = [] }
}

async function saveKit(): Promise<void> {
  localError.value = ''
  const name = kitName.value.trim()
  if (!name) { localError.value = 'Введите название brand kit.'; return }
  if (colors.value.some(color => !color.name.trim() || !/^#[0-9a-f]{6}$/i.test(color.value))) { localError.value = 'Укажите название и цвет в формате #RRGGBB.'; return }
  await createBrandKit({
    name,
    colors: colors.value.map(color => ({ name: color.name.trim(), value: color.value.toLowerCase() })),
    fontFiles: [...fontFiles.value], logoFiles: [...logoFiles.value],
  })
  if (!designState.error) { kitName.value = ''; fontFiles.value = []; logoFiles.value = [] }
}

async function removeConfirmed(): Promise<void> {
  const target = confirmDelete.value
  if (!target) return
  closeConfirmation()
  if (target.type === 'template') await deleteTemplate(target.id)
  else await deleteBrandKit(target.id)
}

async function openConfirmation(invoker: EventTarget | null): Promise<void> {
  confirmationInvoker = invoker instanceof HTMLElement ? invoker : null
  await nextTick()
  confirmation.value?.querySelector<HTMLButtonElement>('button')?.focus()
}

function askApply(event: Event, template: { id: string; name: string }): void {
  confirmDelete.value = null; confirmApply.value = template
  void openConfirmation(event.currentTarget)
}
function askDelete(event: Event, target: { type: 'template' | 'kit'; id: string; name: string }): void {
  confirmApply.value = null; confirmDelete.value = target
  void openConfirmation(event.currentTarget)
}
function closeConfirmation(): void {
  confirmApply.value = null; confirmDelete.value = null
  const invoker = confirmationInvoker; confirmationInvoker = null
  void nextTick(() => invoker?.focus())
}
async function applyConfirmed(): Promise<void> {
  const target = confirmApply.value
  closeConfirmation()
  if (target) await instantiateTemplate(target.id)
}
function onConfirmationKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') { event.preventDefault(); closeConfirmation(); return }
  if (event.key !== 'Tab' || !confirmation.value) return
  const controls = [...confirmation.value.querySelectorAll<HTMLElement>('button:not(:disabled)')]
  if (!controls.length) return
  const first = controls[0]!, last = controls.at(-1)!
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
}
</script>

<template>
  <section class="design-hub" aria-labelledby="design-hub-title" :aria-busy="designState.busy">
    <header><div><h2 id="design-hub-title">Шаблоны и Brand kit</h2><p>В статической версии данные сохраняются на этом устройстве.</p></div></header>
    <div role="group" aria-label="Раздел Design Hub">
      <button id="templates-tab" type="button" :aria-pressed="tab === 'templates'" aria-controls="templates-panel" @click="tab = 'templates'">Шаблоны</button>
      <button id="kits-tab" type="button" :aria-pressed="tab === 'kits'" aria-controls="kits-panel" @click="tab = 'kits'">Brand kits</button>
    </div>
    <p v-if="designState.notice" role="status" aria-live="polite">{{ designState.notice }}</p>
    <p v-if="localError || designState.error" class="error" role="alert">{{ localError || designState.error }}</p>

    <div v-show="tab === 'templates'" id="templates-panel" aria-labelledby="templates-tab">
      <form @submit.prevent="saveTemplate">
        <fieldset :disabled="designState.busy || !hasActiveProject">
          <legend>Создать из активного проекта</legend>
          <p v-if="!hasActiveProject">Откройте проект, чтобы создать шаблон.</p>
          <label>Название шаблона <input v-model="templateName" maxlength="100" required></label>
          <h3>Placeholders</h3>
          <p class="hint">Отметьте заменяемые видео, аудио, текст и логотип. Обязательные поля нужно заполнить при применении.</p>
          <fieldset v-for="(item, index) in placeholders" :key="item.key" class="placeholder-row">
            <legend>Placeholder {{ index + 1 }}</legend>
            <label>Название <input v-model="item.label" maxlength="80" :aria-label="`Название placeholder ${index + 1}`"></label>
            <label>Тип <select v-model="item.kind" :aria-label="`Тип placeholder ${index + 1}`"><option value="video">Видео</option><option value="audio">Аудио</option><option value="text">Текст</option><option value="logo">Логотип</option></select></label>
            <label><input v-model="item.required" type="checkbox"> Обязательный</label>
            <button type="button" class="btn ghost sm" :aria-label="`Удалить placeholder ${index + 1}`" @click="removePlaceholder(item.key)">Удалить</button>
          </fieldset>
          <button type="button" class="btn ghost sm" @click="addPlaceholder">Добавить placeholder</button>
          <label v-if="designState.kits.length">Brand kit <select v-model="templateKitId"><option value="">Без Brand kit</option><option v-for="kit in designState.kits" :key="kit.id" :value="kit.id">{{ kit.name }} · rev {{ kit.revision }}</option></select></label>
          <button type="button" class="btn primary" :disabled="!templateName.trim()" @click="saveTemplate">Сохранить шаблон</button>
        </fieldset>
      </form>
      <ul class="design-cards" aria-label="Мои шаблоны">
        <li v-for="template in designState.templates" :key="template.id">
          <div><strong>{{ template.name }}</strong><span>Ревизия {{ template.revision }}</span><p v-if="template.description">{{ template.description }}</p><small>{{ template.placeholders.length }} placeholders<span v-if="template.brandKitPin"> · Brand kit rev {{ template.brandKitPin.revision }}</span></small></div>
          <div><button type="button" class="btn primary sm" :disabled="designState.busy" @click="askApply($event, template)">Применить</button><button type="button" class="btn danger sm" @click="askDelete($event, { type: 'template', id: template.id, name: template.name })">Удалить</button></div>
        </li>
      </ul>
    </div>

    <div v-show="tab === 'kits'" id="kits-panel" aria-labelledby="kits-tab">
      <form @submit.prevent="saveKit">
        <fieldset :disabled="designState.busy"><legend>Создать Brand kit</legend>
          <label>Название <input v-model="kitName" maxlength="100" required aria-label="Название Brand kit"></label>
          <fieldset v-for="(color, index) in colors" :key="color.key"><legend>Цвет {{ index + 1 }}</legend><label>Роль <input v-model="color.name" :aria-label="`Название цвета ${index + 1}`"></label><label>Значение <input v-model="color.value" type="text" pattern="#[0-9A-Fa-f]{6}" :aria-label="`HEX цвета ${index + 1}`"></label><input v-model="color.value" type="color" :aria-label="`Выбрать цвет ${index + 1}`"><button v-if="colors.length > 1" type="button" @click="removeColor(color.key)">Удалить цвет</button></fieldset>
          <button type="button" class="btn ghost sm" @click="addColor">Добавить цвет</button>
          <label>Шрифты <input type="file" multiple accept=".woff2,.woff,font/woff2,font/woff" @change="fontFiles = files($event)"></label><small>{{ fontFiles.map(file => file.name).join(', ') || 'Файлы не выбраны' }}</small>
          <label>Логотипы <input type="file" multiple accept=".png,.webp,image/png,image/webp" @change="logoFiles = files($event)"></label><small>{{ logoFiles.map(file => file.name).join(', ') || 'Файлы не выбраны' }}</small>
          <button type="button" class="btn primary" :disabled="!kitName.trim()" @click="saveKit">Сохранить Brand kit</button>
        </fieldset>
      </form>
      <ul class="design-cards" aria-label="Brand kits"><li v-for="kit in designState.kits" :key="kit.id"><div><strong>{{ kit.name }}</strong><span>Ревизия {{ kit.revision }}</span><small>{{ kit.colors.length }} цветов · {{ kit.fonts.length }} шрифтов · {{ kit.logos.length }} логотипов</small></div><button type="button" class="btn danger sm" @click="askDelete($event, { type: 'kit', id: kit.id, name: kit.name })">Удалить</button></li></ul>
    </div>

    <div v-if="confirmApply" ref="confirmation" role="alertdialog" aria-modal="true" aria-labelledby="design-apply-title" aria-describedby="design-apply-description" class="delete-confirmation" @keydown="onConfirmationKeydown"><strong id="design-apply-title">Создать новый проект из «{{ confirmApply.name }}»?</strong><p id="design-apply-description">{{ props.hasActiveProject ? 'Текущий проект будет закрыт и заменён новым проектом из шаблона. Убедитесь, что текущие изменения сохранены.' : 'Будет создан новый проект из выбранного шаблона.' }}</p><button type="button" @click="closeConfirmation">Отмена</button><button type="button" class="btn primary" @click="applyConfirmed">Создать новый проект</button></div>
    <div v-else-if="confirmDelete" ref="confirmation" role="alertdialog" aria-modal="true" aria-labelledby="design-delete-title" aria-describedby="design-delete-description" class="delete-confirmation" @keydown="onConfirmationKeydown"><strong id="design-delete-title">Удалить «{{ confirmDelete.name }}»?</strong><p id="design-delete-description">Это действие нельзя отменить.</p><button type="button" @click="closeConfirmation">Отмена</button><button type="button" class="btn danger" @click="removeConfirmed">Удалить</button></div>
  </section>
</template>

<style scoped>
.design-hub { display:grid; gap:.8rem; margin:1rem 0; padding:1rem; border:1px solid var(--border); border-radius:.8rem; }
.design-hub h2,.design-hub p { margin:0; }.design-hub form,.design-hub fieldset { display:grid; gap:.6rem; }.design-hub [role=tablist] { display:flex; gap:.4rem; }
.design-hub [aria-pressed=true] { background:var(--accent); color:white; }.design-cards { display:grid; gap:.55rem; padding:0; list-style:none; }.design-cards li { display:flex; justify-content:space-between; gap:.8rem; padding:.7rem; border:1px solid var(--border); border-radius:.6rem; }.design-cards li>div { display:grid; gap:.2rem; }.placeholder-row,.delete-confirmation { border:1px solid var(--border); border-radius:.55rem; padding:.65rem; }
</style>
