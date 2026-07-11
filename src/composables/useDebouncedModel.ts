import { ref, onBeforeUnmount, type Ref } from 'vue';

/** Tracks a local text value and emits a debounced commit. `value` updates
 *  immediately on input (bind the input's `:value` to it); the `onCommit`
 *  callback fires once typing has been quiet for `delay` ms. `clear()`
 *  bypasses the debounce (immediate empty commit) for the clear (×) button.
 *
 *  Programmatic changes to the value (parent syncing `modelValue`) should set
 *  `value.value` directly rather than call `onInput`, so they don't echo back
 *  as a debounced commit. */
export function useDebouncedModel(
  initial: string,
  onCommit: (v: string) => void,
  delay = 500,
): {
  value: Ref<string>;
  onInput: (e: Event) => void;
  clear: () => void;
} {
  const value = ref(initial);
  let timer: ReturnType<typeof setTimeout> | null = null;

  function flush(raw: string) {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      onCommit(raw);
    }, delay);
  }

  /** Input-event handler: capture the new text, stash it on `value`, and arm
   *  the debounced commit. */
  function onInput(e: Event) {
    value.value = (e.target as HTMLInputElement).value;
    flush(value.value);
  }

  /** Clear immediately (no debounce) and commit the empty value. */
  function clear() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    value.value = '';
    onCommit('');
  }

  onBeforeUnmount(() => {
    if (timer) clearTimeout(timer);
  });

  return { value, onInput, clear };
}
