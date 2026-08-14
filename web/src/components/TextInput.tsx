import clsx from 'clsx';
import { X } from 'lucide-react';
import type { KeyboardEvent, ReactNode } from 'react';

export interface TextInputProps {
  value: string;
  onChange: (value: string) => void;
  /** Required: these inputs are icon-labelled, so nothing else names them. */
  ariaLabel: string;
  placeholder?: string;
  /** Leading affordance, e.g. a lucide icon. */
  icon?: ReactNode;
  /** Given, a clear button appears while the field is non-empty. */
  onClear?: () => void;
  align?: 'left' | 'center';
  size?: 'sm' | 'md';
  inputMode?: 'text' | 'numeric';
  onBlur?: () => void;
  onKeyDown?: (e: KeyboardEvent<HTMLInputElement>) => void;
  className?: string;
}

/** Bordered mono text field with optional leading icon and clear button. */
export function TextInput({
  value,
  onChange,
  ariaLabel,
  placeholder,
  icon,
  onClear,
  align = 'left',
  size = 'md',
  inputMode,
  onBlur,
  onKeyDown,
  className,
}: TextInputProps) {
  return (
    <div
      className={clsx(
        'flex items-center gap-2 border border-idle bg-bg text-body text-xs',
        size === 'sm' ? 'px-1' : 'px-2 py-1',
        className,
      )}
    >
      {icon}

      <input
        type="text"
        inputMode={inputMode}
        aria-label={ariaLabel}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        className={clsx(
          'min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-dim',
          align === 'center' && 'text-center',
        )}
      />

      {onClear && value !== '' && (
        <button
          type="button"
          aria-label={`Clear ${ariaLabel.toLowerCase()}`}
          onClick={onClear}
          className="mco-reset text-dim hover:text-orange"
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}
