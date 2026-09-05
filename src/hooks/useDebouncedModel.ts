import { useState, useRef, useEffect, useCallback } from 'react';

export function useDebouncedModel(
  initial: string,
  onCommit: (v: string) => void,
  delay = 500,
) {
  const [value, setValue] = useState(initial);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onCommitRef = useRef(onCommit);
  onCommitRef.current = onCommit;

  const armTimer = useCallback((v: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      onCommitRef.current(v);
    }, delay);
  }, [delay]);

  const onInput = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setValue(v);
    armTimer(v);
  }, [armTimer]);

  const clear = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setValue('');
    onCommitRef.current('');
  }, []);

  // Sync external changes
  useEffect(() => {
    setValue(initial);
  }, [initial]);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return { value, onInput, clear };
}