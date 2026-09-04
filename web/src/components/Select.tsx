import clsx from 'clsx';

export interface SelectOption<T extends string> {
  value: T;
  label: string;
}

export interface SelectProps<T extends string> {
  label: string;
  value: T;
  options: readonly SelectOption<T>[];
  onChange: (value: T) => void;
  orientation?: 'horizontal' | 'vertical';
}

/** Labeled, mono-styled `<select>`. Generic over the option value union. */
export function Select<T extends string>({
  label,
  value,
  options,
  onChange,
  orientation = 'horizontal',
}: SelectProps<T>) {
  return (
    <label
      className={clsx(
        'flex text-xs text-dim tracking-[0.12em]',
        orientation === 'vertical'
          ? 'flex-col items-start gap-1'
          : 'items-center gap-2',
      )}
    >
      {label}
      <select
        className="border border-idle bg-bg px-1.5 py-0.75 font-mono text-ink text-xs"
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
