<template>
  <Teleport to="body">
    <div class="drawer-overlay" :class="{ 'drawer-overlay--visible': visible }" @click="emitClose" />
    <aside class="collection-drawer" :class="{ 'collection-drawer--open': visible }">
      <h2 class="drawer-title">{{ t('lib.collections.title') }}</h2>

      <div ref="listRef" class="drawer-list" :class="{ 'drawer-list--masked': !!renamingId }">
        <!-- Rename mask: dims everything except the active row -->
        <div v-if="renamingId" class="drawer-list__mask" />

        <!-- "All" — always first, not deletable -->
        <div
          class="drawer-item drawer-item--all"
          :class="{
            'drawer-item--active': store.isAllActive,
            'drawer-item--dimmed': !!renamingId,
          }"
          @click="selectAll"
        >
          <span class="drawer-item__name">{{ t('lib.collections.all') }}</span>
          <MdiIcon
            :path="mdiPin"
            :size="16"
            class="drawer-item__pin"
            aria-hidden="true"
          />
          <span class="drawer-item__count">{{ totalBookCount }}</span>
        </div>

        <!-- User collections -->
        <div
          v-for="col in store.collections"
          :key="col.id"
          :id="'collection-item-' + col.id"
          class="drawer-item"
          :class="{
            'drawer-item--active': store.activeCollectionId === col.id,
            'drawer-item--renaming': renamingId === col.id,
            'drawer-item--dimmed': !!renamingId && renamingId !== col.id,
          }"
          @click="selectCollection(col.id)"
          @contextmenu.prevent.stop="startRename(col)"
        >

          <!-- Inline rename — visually identical to the static name -->
          <input
            v-if="renamingId === col.id"
            ref="renameInputRef"
            v-model="renameValue"
            class="drawer-item__input"
            maxlength="60"
            @blur="commitRename(col)"
            @keydown.enter="commitRename(col)"
            @keydown.escape="cancelRename"
            @click.stop
          />
          <span v-else class="drawer-item__name">{{ col.name }}</span>

          <span class="drawer-item__count">{{ collectionCounts[col.id] ?? 0 }}</span>
        </div>

        <div v-if="!store.collections.length" class="drawer-empty">
          {{ t('lib.collections.empty') }}
        </div>
      </div>

      <!-- Bottom bar: + when idle, delete button appears when renaming -->
      <div class="drawer-fab-wrap" :class="{ 'drawer-fab-wrap--elevated': !!renamingId }">
        <button
          v-if="renamingId"
          class="drawer-fab drawer-fab--delete"
          @click="onDeleteClick"
        >
          <MdiIcon :path="mdiDelete" :size="24" />
        </button>
        <button
          v-else
          class="drawer-fab"
          @click="onCreate"
        >
          <MdiIcon :path="mdiPlus" :size="24" />
        </button>
      </div>

      <!-- Delete confirmation dialog -->
      <md-dialog
        ref="dialogRef"
        class="delete-dialog"
        @close="cancelDelete"
      >
        <div slot="headline">{{ t('lib.collections.delete') }}</div>
        <div slot="content" class="delete-dialog__content">
          {{ t('lib.collections.confirmDelete', { name: deleteTargetName }) }}
        </div>
        <div slot="actions">
          <md-text-button @click="cancelDelete">{{ t('common.cancel') }}</md-text-button>
          <md-filled-button @click="confirmDelete">{{ t('common.confirm') }}</md-filled-button>
        </div>
      </md-dialog>
    </aside>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, reactive, nextTick, onMounted, watch } from 'vue';
import {
  mdiPlus,
  mdiDelete,
  mdiPin,
} from '@mdi/js';
import { useCollectionsStore } from '@/stores/collections';
import { useToastStore } from '@/stores/toast';
import { api } from '@/services/api';
import { useI18n } from '@/i18n';
import MdiIcon from '@/components/MdiIcon.vue';
import type { Collection } from '@/types';

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void; (e: 'close'): void }>();

const store = useCollectionsStore();
const toast = useToastStore();
const { t } = useI18n();

const visible = ref(false);

/* ---- State ---- */
const listRef = ref<HTMLElement | null>(null);
const renamingId = ref<string | null>(null);
const renameValue = ref('');
const renameInputRef = ref<HTMLInputElement | null>(null);

/* ---- Delete confirmation dialog ---- */
const deleteTargetId = ref<string | null>(null);
const deleteTargetName = ref('');
const dialogRef = ref<{ show: () => void; close: () => void } | null>(null);

/** Book counts per collection (id → count). */
const collectionCounts = reactive<Record<string, number>>({});

/** Total book count shown next to "All". */
const totalBookCount = ref(0);

/* ---- Lifecycle ---- */

onMounted(() => {
  store.ensureLoaded();
  refreshCounts();
});

watch(() => props.modelValue, (v) => {
  visible.value = v;
  if (v) {
    store.ensureLoaded();
    refreshCounts();
  }
}, { immediate: true });

async function refreshCounts() {
  api.searchBooks({ sort_by: 'date', sort_order: 'desc', page: 1, page_size: 1 })
    .then(r => { totalBookCount.value = r.total; })
    .catch(() => {});

  for (const col of store.collections) {
    api.searchBooks({
      sort_by: 'date',
      sort_order: 'desc',
      page: 1,
      page_size: 1,
      collections: [col.name],
    }).then(r => { collectionCounts[col.id] = r.total; })
      .catch(() => {});
  }
}

/* ---- Open / close ---- */

function emitClose() {
  visible.value = false;
  emit('update:modelValue', false);
  emit('close');
}

/* ---- Selection ---- */

function selectAll() {
  store.setActiveCollection(null);
  emitClose();
}

function selectCollection(id: string) {
  if (renamingId.value) return; // block clicks while renaming
  store.setActiveCollection(id);
  emitClose();
}

/* ---- Inline rename ---- */

function startRename(col: { id: string; name: string }) {
  renamingId.value = col.id;
  renameValue.value = col.name;
  // Double nextTick: first for v-if to create the input, second for focus
  void nextTick(() => {
    void nextTick(() => {
      renameInputRef.value?.focus();
      renameInputRef.value?.select();
    });
  });
}

async function commitRename(col: { id: string; name: string }) {
  const name = renameValue.value.trim();
  renamingId.value = null;
  if (name && name !== col.name) {
    await store.renameCollection(col.id, name);
  }
}

function cancelRename() {
  renamingId.value = null;
}

/* ---- Delete (triggered from bottom button when renaming) ---- */

function onDeleteClick() {
  if (!renamingId.value) return;
  const col = store.collections.find(c => c.id === renamingId.value);
  if (!col) return;
  deleteTargetId.value = col.id;
  deleteTargetName.value = col.name;
  dialogRef.value?.show();
}

async function confirmDelete() {
  const id = deleteTargetId.value;
  const name = deleteTargetName.value;
  deleteTargetId.value = null;
  deleteTargetName.value = '';
  renamingId.value = null;
  dialogRef.value?.close();
  if (id) {
    const ok = await store.deleteCollection(id);
    if (ok) {
      toast.addToast('success', t('lib.collections.deleted', { name }));
    }
  }
}

function cancelDelete() {
  deleteTargetId.value = null;
  deleteTargetName.value = '';
  dialogRef.value?.close();
}

/* ---- Create ---- */

const MAX_COLLECTIONS = 100;

async function onCreate() {
  if (store.collections.length >= MAX_COLLECTIONS) {
    toast.addToast('error', t('lib.collections.maxReached', { max: MAX_COLLECTIONS }));
    return;
  }

  const baseName = t('lib.collections.defaultName');
  // Find an unused index starting from 1
  const existingNames = new Set(store.collections.map(c => c.name));
  let name = baseName;
  for (let i = 1; existingNames.has(name); i++) {
    name = `${baseName} ${i}`;
  }

  const col = await store.createCollection(name);
  if (col) {
    toast.addToast('success', t('lib.collections.created', { name }));
  }
  setTimeout(refreshCounts, 100);
}
</script>

<style scoped>
.drawer-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(0, 0, 0, 0.4);
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.25s ease;
}
.drawer-overlay--visible {
  opacity: 1;
  pointer-events: auto;
}

.collection-drawer {
  position: fixed;
  top: 0;
  right: 0;
  bottom: 0;
  width: min(320px, 85vw);
  z-index: 101;
  background: var(--md-sys-color-surface-container-low);
  color: var(--md-sys-color-on-surface);
  display: flex;
  flex-direction: column;
  box-shadow: var(--md-sys-elevation-level3);
  transform: translateX(100%);
  transition: transform 0.25s ease;
}
.collection-drawer--open {
  transform: translateX(0);
}

.drawer-title {
  margin: 0;
  padding: 24px 20px 16px;
  font: 400 var(--md-sys-typescale-headline-small-size) /
    var(--md-sys-typescale-headline-small-line-height)
    var(--md-sys-typescale-font);
  flex-shrink: 0;
}

.drawer-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 12px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  position: relative;
}

/* Mask: semi-transparent scrim over the list when renaming */
.drawer-list__mask {
  position: absolute;
  inset: 0;
  z-index: 2;
  background: color-mix(in srgb, var(--md-sys-color-surface-container-low) 60%, transparent);
  pointer-events: auto;
  border-radius: 8px;
}

.drawer-item {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: var(--md-sys-shape-corner-medium);
  cursor: pointer;
  user-select: none;
  transition: background-color 0.15s ease, opacity 0.2s ease;
}
.drawer-item:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
}
.drawer-item--active {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}
.drawer-item--active:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-secondary-container) 12%, var(--md-sys-color-secondary-container));
}

/* Renaming row pops above the mask */
.drawer-item--renaming {
  z-index: 3;
  background: var(--md-sys-color-surface-container-low);
  border-radius: var(--md-sys-shape-corner-medium);
}

/* Dimmed items under the mask */
.drawer-item--dimmed {
  opacity: 0.4;
  pointer-events: none;
}

/* Pin icon next to "All" */
.drawer-item__pin {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  color: var(--md-sys-color-on-surface-variant);
  opacity: 0.4;
}

.drawer-item__name {
  flex: 1;
  font: var(--md-sys-typescale-body-large-weight)
    var(--md-sys-typescale-body-large-size) /
    var(--md-sys-typescale-body-large-line-height)
    var(--md-sys-typescale-font);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.drawer-item__count {
  font: var(--md-sys-typescale-label-medium-weight)
    var(--md-sys-typescale-label-medium-size) /
    var(--md-sys-typescale-label-medium-line-height)
    var(--md-sys-typescale-font);
  color: var(--md-sys-color-on-surface-variant);
  background: var(--md-sys-color-surface-container-highest);
  padding: 2px 8px;
  border-radius: var(--md-sys-shape-corner-full);
}

/* Inline rename input — matches .drawer-item__name exactly */
.drawer-item__input {
  flex: 1;
  padding: 0;
  margin: 0;
  border: none;
  border-radius: 0;
  background: transparent;
  color: inherit;
  font: var(--md-sys-typescale-body-large-weight)
    var(--md-sys-typescale-body-large-size) /
    var(--md-sys-typescale-body-large-line-height)
    var(--md-sys-typescale-font);
  outline: none;
  min-width: 0;
}
.drawer-item__input:focus {
  caret-color: var(--md-sys-color-primary);
}

.drawer-item--all {
  cursor: pointer;
}

.drawer-empty {
  padding: 24px 12px;
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
  font: var(--md-sys-typescale-body-medium-weight)
    var(--md-sys-typescale-body-medium-size) /
    var(--md-sys-typescale-body-medium-line-height)
    var(--md-sys-typescale-font);
}

.drawer-fab-wrap {
  display: flex;
  justify-content: center;
  padding: 16px 20px 20px;
  flex-shrink: 0;
  position: relative;
  z-index: 1;
}
.drawer-fab-wrap--elevated {
  z-index: 101;
}

.drawer-fab {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
  box-shadow: var(--md-sys-elevation-level3);
  cursor: pointer;
  transition: box-shadow 0.15s ease, transform 0.15s ease, background-color 0.2s ease;
}
.drawer-fab:hover {
  box-shadow: var(--md-sys-elevation-level4);
  transform: scale(1.05);
}

.drawer-fab--delete {
  background: var(--md-sys-color-error);
  color: var(--md-sys-color-on-error);
}

.delete-dialog {
  --md-dialog-container-color: var(--md-sys-color-surface-container-high);
}

.delete-dialog__content {
  font: var(--md-sys-typescale-body-medium-weight)
    var(--md-sys-typescale-body-medium-size) /
    var(--md-sys-typescale-body-medium-line-height)
    var(--md-sys-typescale-font);
}
</style>
