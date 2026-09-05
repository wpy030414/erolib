import { useEffect } from 'react';
import { MdiIcon } from './MdiIcon';
import { useDebouncedModel } from '@/hooks/useDebouncedModel';
import { mdiMagnify, mdiClose } from '@mdi/js';

interface SearchBoxProps {
  value: string;
  placeholder?: string;
  clearLabel?: string;
  debounce?: number;
  onChange: (v: string) => void;
  onCommit: (v: string) => void;
}

export function SearchBox({
  value,
  placeholder,
  clearLabel,
  debounce = 500,
  onChange,
  onCommit,
}: SearchBoxProps) {
  const { value: localValue, onInput, clear } = useDebouncedModel(
    value,
    onCommit,
    debounce,
  );

  // Sync local value to parent on each keystroke
  useEffect(() => {
    onChange(localValue);
  }, [localValue, onChange]);

  function handleClear() {
    onChange('');
    clear();
  }

  return (
    <div className="search-box">
      <MdiIcon path={mdiMagnify} size={18} />
      <input
        className="search-input"
        type="search"
        value={localValue}
        placeholder={placeholder}
        onChange={onInput}
      />
      {localValue && (
        <button
          className="search-clear"
          aria-label={clearLabel}
          onClick={handleClear}
        >
          <MdiIcon path={mdiClose} size={16} />
        </button>
      )}
    </div>
  );
}