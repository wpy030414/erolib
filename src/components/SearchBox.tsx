import { useState, useRef, useEffect, useCallback } from 'react';
import { MdiIcon } from './MdiIcon';
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
  const [localValue, setLocalValue] = useState(value);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onChangeRef = useRef(onChange);
  const onCommitRef = useRef(onCommit);
  onChangeRef.current = onChange;
  onCommitRef.current = onCommit;

  // Sync external value changes (but NOT from our own onChange calls)
  const externalRef = useRef(value);
  useEffect(() => {
    if (value !== externalRef.current) {
      externalRef.current = value;
      setLocalValue(value);
    }
  }, [value]);

  const armDebounce = useCallback((v: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      onCommitRef.current(v);
    }, debounce);
  }, [debounce]);

  const handleInput = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setLocalValue(v);
    onChangeRef.current(v);
    armDebounce(v);
  }, [armDebounce]);

  const handleClear = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setLocalValue('');
    onChangeRef.current('');
    onCommitRef.current('');
  }, []);

  useEffect(() => {
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, []);

  return (
    <div className="search-box">
      <MdiIcon path={mdiMagnify} size={18} />
      <input
        className="search-input"
        type="search"
        value={localValue}
        placeholder={placeholder}
        onChange={handleInput}
      />
      {localValue && (
        <button className="search-clear" aria-label={clearLabel} onClick={handleClear}>
          <MdiIcon path={mdiClose} size={16} />
        </button>
      )}
    </div>
  );
}