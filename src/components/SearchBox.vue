<template>
  <div class="search-box">
    <MdiIcon :path="mdiMagnify" :size="18" class="search-icon" />
    <input
      class="search-input"
      type="search"
      :value="value"
      :placeholder="placeholder"
      @input="handleInput"
    />
    <button
      v-if="value"
      class="search-clear"
      :aria-label="clearLabel"
      @click="handleClear"
    >
      <MdiIcon :path="mdiClose" :size="16" />
    </button>
  </div>
</template>

<script setup lang="ts">
import { watch } from 'vue';
import { mdiMagnify, mdiClose } from '@mdi/js';
import MdiIcon from '@/components/MdiIcon.vue';
import { useDebouncedModel } from '@/composables/useDebouncedModel';

/**
 * Unified search box (Library / Pixiv / EHentai). `v-model` mirrors the live
 * text; `@commit` fires (debounced) when typing settles, or immediately from
 * the clear (×) button. Styling is the global `.search-box` set in md3.css. */
const props = withDefaults(
  defineProps<{
    modelValue?: string;
    placeholder?: string;
    clearLabel?: string;
    debounce?: number;
  }>(),
  { modelValue: '', debounce: 500 },
);

const emit = defineEmits<{
  (e: 'update:modelValue', v: string): void;
  (e: 'commit', v: string): void;
}>();

const { value, onInput, clear } = useDebouncedModel(
  props.modelValue,
  (v) => emit('commit', v),
  props.debounce,
);

function handleInput(e: Event) {
  onInput(e);
  // Live mirror so v-model callers (Library) see each keystroke.
  emit('update:modelValue', value.value);
}

function handleClear() {
  // Update the model before firing commit, so v-model callers that read their
  // own state inside @commit (Library's applySearch reads query.value) observe
  // the cleared value rather than the previous one.
  emit('update:modelValue', '');
  clear();
}

// Programmatic parent change → sync local text WITHOUT re-emitting commit.
watch(
  () => props.modelValue,
  (v) => {
    if (v !== value.value) value.value = v;
  },
);
</script>
