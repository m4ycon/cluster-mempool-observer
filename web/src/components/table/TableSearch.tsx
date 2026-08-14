import { Search } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useDebouncedCallback } from 'use-debounce';
import { TextInput } from '../TextInput';

export interface TableSearchProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

const DEBOUNCE_MS = 150;

/** Debounced search box: local draft for instant feedback, `onChange` fires 150ms after typing settles. */
export function TableSearch({
  value,
  onChange,
  placeholder,
}: TableSearchProps) {
  const [draft, setDraft] = useState(value);

  // Follow an external reset (back button, cleared filter elsewhere).
  useEffect(() => {
    setDraft(value);
  }, [value]);

  const commit = useDebouncedCallback(onChange, DEBOUNCE_MS);

  const handleChange = (next: string) => {
    setDraft(next);
    commit(next);
  };

  const handleClear = () => {
    setDraft('');
    commit.cancel();
    onChange('');
  };

  return (
    <TextInput
      value={draft}
      onChange={handleChange}
      onClear={handleClear}
      ariaLabel="Search"
      placeholder={placeholder}
      icon={<Search size={14} className="text-dim" />}
    />
  );
}
