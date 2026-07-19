<template>
  <md-dialog
    ref="dialogRef"
    class="book-collection-picker"
    @close="onClose"
  >
    <div slot="headline">{{ t('lib.collections.addToTitle') }}</div>
    <div slot="content" class="picker__content">
      <div
        v-if="!store.collections.length"
        class="picker__empty"
      >
        {{ t('lib.collections.empty') }}
      </div>
      <label
        v-for="col in store.collections"
        :key="col.id"
        class="picker__row"
      >
        <input
          type="checkbox"
          class="picker__checkbox"
          :checked="checkedIds.has(col.id)"
          @change="toggle(col.id)"
        />
        <span class="picker__name">{{ col.name }}</span>
      </label>
    </div>
    <div slot="actions">
      <md-filled-button @click="close">{{ t('common.confirm') }}</md-filled-button>
    </div>
  </md-dialog>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useCollectionsStore } from '@/stores/collections';
import { useI18n } from '@/i18n';

const props = defineProps<{ bookId: string }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const store = useCollectionsStore();
const { t } = useI18n();

const dialogRef = ref<{ show: () => void; close: () => void } | null>(null);
const checkedIds = ref<Set<string>>(new Set());
/** Snapshot of checked IDs at dialog open — used to compute deltas on close. */
const initialIds = ref<Set<string>>(new Set());

onMounted(async () => {
  store.ensureLoaded();
  const ids = await store.getBookCollections(props.bookId);
  checkedIds.value = new Set(ids);
  initialIds.value = new Set(ids);
  dialogRef.value?.show();
});

function toggle(collectionId: string) {
  const next = new Set(checkedIds.value);
  if (next.has(collectionId)) {
    next.delete(collectionId);
  } else {
    next.add(collectionId);
  }
  checkedIds.value = next;
}

async function onClose() {
  const added: string[] = [];
  const removed: string[] = [];
  for (const id of checkedIds.value) {
    if (!initialIds.value.has(id)) added.push(id);
  }
  for (const id of initialIds.value) {
    if (!checkedIds.value.has(id)) removed.push(id);
  }
  await Promise.all([
    ...added.map((cid) => store.addBookToCollection(cid, props.bookId)),
    ...removed.map((cid) => store.removeBookFromCollection(cid, props.bookId)),
  ]);
  emit('close');
}

function close() {
  dialogRef.value?.close();
}
</script>

<style scoped>
.book-collection-picker {
  --md-dialog-container-color: var(--md-sys-color-surface-container-high);
}

.picker__content {
  min-width: min(300px, 70vw);
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: min(360px, 50vh);
  overflow-y: auto;
}

.picker__row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: var(--md-sys-shape-corner-medium);
  cursor: pointer;
  transition: background-color 0.15s ease;
}

.picker__row:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
}

.picker__checkbox {
  width: 18px;
  height: 18px;
  accent-color: var(--md-sys-color-primary);
  cursor: pointer;
  flex-shrink: 0;
}

.picker__name {
  font: var(--md-sys-typescale-body-large-weight)
    var(--md-sys-typescale-body-large-size) /
    var(--md-sys-typescale-body-large-line-height)
    var(--md-sys-typescale-font);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.picker__empty {
  padding: 16px;
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
  font: var(--md-sys-typescale-body-medium-weight)
    var(--md-sys-typescale-body-medium-size) /
    var(--md-sys-typescale-body-medium-line-height)
    var(--md-sys-typescale-font);
}
</style>
