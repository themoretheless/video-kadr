import { migrateProjectDocument, type JsonObject, type ProjectDocument, validateProjectDocument } from '../project-schema'
import { migrateBrandKit, type BrandKit } from './brand-kit'

export const PROJECT_TEMPLATE_SCHEMA_VERSION = 1 as const
export type TemplatePlaceholder =
  | { id: string; name: string; kind: 'media'; target: { mediaId: string }; required: boolean; constraints?: { mediaKind?: 'video' | 'audio' | 'image'; maxBytes?: number } }
  | { id: string; name: string; kind: 'text'; target: { effectId: string; parameter: 'text' }; required: boolean; constraints?: { maxLength?: number } }
  | { id: string; name: string; kind: 'logo'; target: { effectId: string; parameter: 'assetRef' }; required: boolean; constraints?: { mimeTypes?: Array<'image/png' | 'image/webp' | 'image/svg+xml'>; maxBytes?: number } }

export interface ProjectTemplate {
  schemaVersion: typeof PROJECT_TEMPLATE_SCHEMA_VERSION
  id: string; revision: number; name: string; description?: string
  sourceProjectSchemaVersion: 4
  document: ProjectDocument
  placeholders: TemplatePlaceholder[]
  brandKitPin?: { id: string; revision: number }
  brandKitSnapshot?: BrandKit
  requiredAssets: Array<{ kind: 'media' | 'font' | 'logo'; assetRef: string; fingerprint: string; mimeType: string; byteLength: number }>
}
export type PlaceholderBinding =
  | { kind: 'media'; media: { assetRef: string; contentFingerprint: string; kind: 'video' | 'audio' | 'image'; metadata: JsonObject } }
  | { kind: 'text'; text: string }
  | { kind: 'logo'; assetRef: string; fingerprint: string; mimeType: 'image/png' | 'image/webp' | 'image/svg+xml'; byteLength: number }

const ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/; const SHA = /^[a-f0-9]{64}$/
const rec = (v: unknown, f: string): Record<string, unknown> => { if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error(`invalid ${f}`); return v as Record<string, unknown> }
const exact = (v: Record<string, unknown>, a: string[], f: string) => { const k = Object.keys(v).find(x => !a.includes(x)); if (k) throw new Error(`invalid ${f}: unexpected ${k}`) }
const validId: (v: unknown, f: string) => asserts v is string = (v, f) => { if (typeof v !== 'string' || !ID.test(v)) throw new Error(`invalid ${f}`) }
const label: (v: unknown, f: string) => asserts v is string = (v, f) => { if (typeof v !== 'string' || v !== v.trim() || !v || [...v].length > 256 || /\p{Cc}/u.test(v)) throw new Error(`invalid ${f}`) }

export function validateProjectTemplate(value: ProjectTemplate): void {
  const root = rec(value, 'project template'); exact(root, ['schemaVersion','id','revision','name','description','sourceProjectSchemaVersion','document','placeholders','brandKitPin','brandKitSnapshot','requiredAssets'], 'project template')
  if (value.schemaVersion !== 1) throw new Error(`unsupported project template schemaVersion ${String(value.schemaVersion)}`)
  validId(value.id, 'template id'); label(value.name, 'template name')
  if (!Number.isSafeInteger(value.revision) || value.revision < 1 || value.sourceProjectSchemaVersion !== 4 || (value.description !== undefined && (typeof value.description !== 'string' || value.description.length > 4096))) throw new Error('invalid project template')
  validateProjectDocument(value.document)
  if (!Array.isArray(value.placeholders) || value.placeholders.length > 256 || !Array.isArray(value.requiredAssets) || value.requiredAssets.length > 512) throw new Error('invalid template collections')
  const media = new Map(value.document.media.map(x => [x.id, x])); const effects = new Map(value.document.sequences.flatMap(s => s.tracks).flatMap(t => t.clips).flatMap(c => c.effects).map(e => [e.id, e])); const ids = new Set<string>(); const targets = new Set<string>()
  for (const p of value.placeholders) {
    const v = rec(p, 'placeholder'); exact(v, ['id','name','kind','target','required','constraints'], 'placeholder'); validId(p.id, 'placeholder id'); label(p.name, 'placeholder name'); if (ids.has(p.id)) throw new Error(`duplicate placeholder id ${p.id}`); ids.add(p.id); if (typeof p.required !== 'boolean') throw new Error('invalid placeholder required')
    const target = rec(p.target, 'placeholder target'); const constraints = p.constraints === undefined ? undefined : rec(p.constraints, 'placeholder constraints')
    let key: string
    if (p.kind === 'media') { exact(target, ['mediaId'], 'media target'); validId(p.target.mediaId, 'media target'); if (!media.has(p.target.mediaId)) throw new Error(`missing placeholder target ${p.target.mediaId}`); if (constraints) { exact(constraints, ['mediaKind','maxBytes'], 'media constraints'); if (p.constraints?.mediaKind !== undefined && !['video','audio','image'].includes(p.constraints.mediaKind) || p.constraints?.maxBytes !== undefined && (!Number.isSafeInteger(p.constraints.maxBytes) || p.constraints.maxBytes <= 0)) throw new Error('invalid media constraints') }; key=`media:${p.target.mediaId}` }
    else if (p.kind === 'text') { exact(target, ['effectId','parameter'], 'text target'); validId(p.target.effectId, 'text target'); if (p.target.parameter !== 'text' || !effects.has(p.target.effectId)) throw new Error('invalid text target'); if (constraints) { exact(constraints, ['maxLength'], 'text constraints'); if (p.constraints?.maxLength !== undefined && (!Number.isInteger(p.constraints.maxLength) || p.constraints.maxLength < 1 || p.constraints.maxLength > 10000)) throw new Error('invalid text constraints') }; key=`effect:${p.target.effectId}:text` }
    else if (p.kind === 'logo') { exact(target, ['effectId','parameter'], 'logo target'); validId(p.target.effectId, 'logo target'); if (p.target.parameter !== 'assetRef' || !effects.has(p.target.effectId)) throw new Error('invalid logo target'); if (constraints) { exact(constraints, ['mimeTypes','maxBytes'], 'logo constraints'); if (p.constraints?.mimeTypes !== undefined && (!Array.isArray(p.constraints.mimeTypes) || p.constraints.mimeTypes.length < 1 || p.constraints.mimeTypes.some(x => !['image/png','image/webp','image/svg+xml'].includes(x))) || p.constraints?.maxBytes !== undefined && (!Number.isSafeInteger(p.constraints.maxBytes) || p.constraints.maxBytes <= 0)) throw new Error('invalid logo constraints') }; key=`effect:${p.target.effectId}:assetRef` }
    else throw new Error('invalid placeholder kind')
    if (targets.has(key)) throw new Error(`duplicate placeholder target ${key}`); targets.add(key)
  }
  if (value.brandKitPin) { exact(rec(value.brandKitPin, 'brand kit pin'), ['id','revision'], 'brand kit pin'); validId(value.brandKitPin.id, 'brand kit pin id'); if (!Number.isSafeInteger(value.brandKitPin.revision) || value.brandKitPin.revision < 1) throw new Error('invalid brand kit pin') }
  if (value.brandKitSnapshot) {
    validatePinnedSnapshot(value.brandKitPin, value.brandKitSnapshot)
  } else if (value.brandKitPin) throw new Error('missing pinned brand kit snapshot')
  const assetKeys = new Set<string>()
  for (const a of value.requiredAssets) { exact(rec(a, 'required asset'), ['kind','assetRef','fingerprint','mimeType','byteLength'], 'required asset'); validId(a.assetRef, 'required asset ref'); if (!['media','font','logo'].includes(a.kind) || !SHA.test(a.fingerprint) || typeof a.mimeType !== 'string' || !a.mimeType || !Number.isSafeInteger(a.byteLength) || a.byteLength <= 0 || assetKeys.has(`${a.kind}:${a.assetRef}`)) throw new Error('invalid required asset'); assetKeys.add(`${a.kind}:${a.assetRef}`) }
  const required = new Map(value.requiredAssets.map(item => [`${item.kind}:${item.assetRef}`, item]))
  for (const item of value.document.media) {
    const bytes = item.metadata.sizeBytes, mime = typeof item.metadata.fileType === 'string' && item.metadata.fileType ? item.metadata.fileType : 'application/octet-stream'
    const dependency = item.assetRef ? required.get(`media:${item.assetRef}`) : undefined
    if (!item.assetRef || !item.contentFingerprint || typeof bytes !== 'number' || !Number.isSafeInteger(bytes) || bytes <= 0
      || !dependency || dependency.fingerprint !== item.contentFingerprint || dependency.byteLength !== bytes || dependency.mimeType !== mime) throw new Error(`missing required media asset ${item.id}`)
  }
  for (const item of value.brandKitSnapshot?.fonts ?? []) requireBrandAsset(required, 'font', item.asset)
  for (const item of value.brandKitSnapshot?.logos ?? []) requireBrandAsset(required, 'logo', item.asset)
}

function requireBrandAsset(required: Map<string, ProjectTemplate['requiredAssets'][number]>, kind: 'font' | 'logo', asset: BrandKit['fonts'][number]['asset']): void {
  const dependency = required.get(`${kind}:${asset.assetRef}`)
  if (!dependency || dependency.fingerprint !== asset.fingerprint || dependency.mimeType !== asset.mimeType || dependency.byteLength !== asset.byteLength) throw new Error(`missing required ${kind} asset ${asset.assetRef}`)
}

function validatePinnedSnapshot(pin: ProjectTemplate['brandKitPin'], snapshot: BrandKit): void {
  if (!pin) throw new Error('brand kit snapshot requires pin')
  const decoded = migrateBrandKit(snapshot)
  if (decoded.id !== pin.id || decoded.revision !== pin.revision) throw new Error('brand kit snapshot does not match pin')
}

export function migrateProjectTemplate(value: unknown): ProjectTemplate { const raw=rec(value,'project template'); if(raw.schemaVersion!==1) throw new Error(`unsupported project template schemaVersion ${String(raw.schemaVersion)}`); const result=structuredClone(raw) as unknown as ProjectTemplate; result.document=migrateProjectDocument(result.document); validateProjectTemplate(result); return result }
export function nextProjectTemplateRevision(previous: ProjectTemplate, update: Omit<ProjectTemplate,'schemaVersion'|'id'|'revision'>): ProjectTemplate { validateProjectTemplate(previous); if(previous.revision===Number.MAX_SAFE_INTEGER) throw new Error('template revision overflow'); const result={...structuredClone(update),schemaVersion:1,id:previous.id,revision:previous.revision+1} as ProjectTemplate; validateProjectTemplate(result); return result }

export function instantiateProjectTemplate(template: ProjectTemplate, bindings: Readonly<Record<string, PlaceholderBinding>>, makeId: (oldId: string) => string): ProjectDocument {
  validateProjectTemplate(template); const supplied=new Set(Object.keys(bindings)); const document=structuredClone(template.document)
  for(const p of template.placeholders){ const b=bindings[p.id]; if(!b){if(p.required) throw new Error(`missing placeholder binding ${p.id}`); continue} supplied.delete(p.id); if(b.kind!==p.kind) throw new Error(`incompatible placeholder binding ${p.id}`)
    if(p.kind==='media'&&b.kind==='media'){ const m=document.media.find(x=>x.id===p.target.mediaId)!; const bytes=b.media.metadata.sizeBytes; if(!ID.test(b.media.assetRef)||!SHA.test(b.media.contentFingerprint)||(p.constraints?.mediaKind && b.media.kind!==p.constraints.mediaKind)||b.media.kind!==m.kind||p.constraints?.maxBytes!==undefined&&(typeof bytes!=='number'||bytes>p.constraints.maxBytes)) throw new Error(`incompatible media binding ${p.id}`); m.assetRef=b.media.assetRef; m.contentFingerprint=b.media.contentFingerprint; m.metadata=structuredClone(b.media.metadata) }
    else if(p.kind==='text'&&b.kind==='text'){ if(p.constraints?.maxLength!==undefined&&[...b.text].length>p.constraints.maxLength) throw new Error(`text binding too long ${p.id}`); const e=findEffect(document,p.target.effectId); e.parameters.text=b.text }
    else if(p.kind==='logo'&&b.kind==='logo'){ if(!SHA.test(b.fingerprint)||!ID.test(b.assetRef)||b.byteLength<=0||p.constraints?.mimeTypes&&!p.constraints.mimeTypes.includes(b.mimeType)||p.constraints?.maxBytes!==undefined&&b.byteLength>p.constraints.maxBytes) throw new Error(`invalid logo binding ${p.id}`); const e=findEffect(document,p.target.effectId); e.parameters.assetRef=b.assetRef; e.parameters.contentFingerprint=b.fingerprint; e.parameters.mimeType=b.mimeType }
  }
  if(supplied.size) throw new Error(`unknown placeholder binding ${[...supplied][0]}`)
  const maps={media:new Map<string,string>(),sequence:new Map<string,string>(),track:new Map<string,string>(),clip:new Map<string,string>(),effect:new Map<string,string>(),multicam:new Map<string,string>(),angle:new Map<string,string>(),decision:new Map<string,string>()}; const used=new Set<string>()
  const put=(map:Map<string,string>,old:string)=>{const next=makeId(old); validId(next,'generated id'); if(used.has(next)) throw new Error(`duplicate generated id ${next}`); used.add(next); map.set(old,next)}
  document.media.forEach(x=>put(maps.media,x.id)); document.sequences.forEach(s=>{put(maps.sequence,s.id);s.tracks.forEach(t=>{put(maps.track,t.id);t.clips.forEach(c=>{put(maps.clip,c.id);c.effects.forEach(e=>put(maps.effect,e.id))})})}); document.multicamGroups.forEach(g=>{put(maps.multicam,g.id);g.angles.forEach(a=>put(maps.angle,a.id));g.decisions.forEach(d=>put(maps.decision,d.id))})
  document.primaryMediaId=maps.media.get(document.primaryMediaId)!; document.activeSequenceId=maps.sequence.get(document.activeSequenceId)!; document.media.forEach(x=>x.id=maps.media.get(x.id)!); document.sequences.forEach(s=>{s.id=maps.sequence.get(s.id)!;s.tracks.forEach(t=>{t.id=maps.track.get(t.id)!;t.clips.forEach(c=>{c.id=maps.clip.get(c.id)!;c.mediaId=maps.media.get(c.mediaId)!;if(c.multicamGroupId)c.multicamGroupId=maps.multicam.get(c.multicamGroupId)!;c.effects.forEach(e=>e.id=maps.effect.get(e.id)!)})})}); document.multicamGroups.forEach(g=>{g.id=maps.multicam.get(g.id)!;g.referenceAngleId=maps.angle.get(g.referenceAngleId)!;g.audioAngleId=maps.angle.get(g.audioAngleId)!;g.angles.forEach(a=>{a.id=maps.angle.get(a.id)!;a.mediaId=maps.media.get(a.mediaId)!});g.decisions.forEach(d=>{d.id=maps.decision.get(d.id)!;d.angleId=maps.angle.get(d.angleId)!})})
  if (template.brandKitSnapshot) applyBrandKitSnapshot(document, template.brandKitSnapshot)
  validateProjectDocument(document); return document
}
function findEffect(document:ProjectDocument,id:string){const effect=document.sequences.flatMap(s=>s.tracks).flatMap(t=>t.clips).flatMap(c=>c.effects).find(e=>e.id===id);if(!effect)throw new Error(`missing placeholder target ${id}`);return effect}

/** Resolve stable brand token ids into durable values while retaining the pinned snapshot. */
function applyBrandKitSnapshot(document: ProjectDocument, kit: BrandKit): void {
  const snapshot = migrateBrandKit(kit)
  document.brandKitSnapshot = snapshot
  const colors = new Map(snapshot.colors.map(item => [item.id, item]))
  const fonts = new Map(snapshot.fonts.map(item => [item.id, item]))
  const logos = new Map(snapshot.logos.map(item => [item.id, item]))
  for (const effect of document.sequences.flatMap(sequence => sequence.tracks).flatMap(track => track.clips).flatMap(clip => clip.effects)) {
    const colorId = effect.parameters.brandColorId
    if (typeof colorId === 'string') { const color = colors.get(colorId); if (!color) throw new Error(`missing brand color ${colorId}`); effect.parameters.color = structuredClone(color.value) }
    const fontId = effect.parameters.brandFontId
    if (typeof fontId === 'string') { const font = fonts.get(fontId); if (!font) throw new Error(`missing brand font ${fontId}`); effect.parameters.font = { family: font.family, weight: font.weight, style: font.style, ...structuredClone(font.asset) } }
    const logoId = effect.parameters.brandLogoId
    if (typeof logoId === 'string') { const logo = logos.get(logoId); if (!logo) throw new Error(`missing brand logo ${logoId}`); effect.parameters.assetRef = logo.asset.assetRef; effect.parameters.contentFingerprint = logo.asset.fingerprint; effect.parameters.mimeType = logo.asset.mimeType; effect.parameters.byteLength = logo.asset.byteLength }
  }
}
