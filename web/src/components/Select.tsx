export interface SelectOption<T extends string> {
  value: T;
  label: string;
}

export interface SelectProps<T extends string> {
  label: string;
  value: T;
  options: readonly SelectOption<T>[];
  onChange: (value: T) => void;
}

/** Labeled, mono-styled `<select>`. Generic over the option value union. */
export function Select<T extends string>({
  label,
  value,
  options,
  onChange,
}: SelectProps<T>) {
  return (
    <label className="flex items-center gap-2 text-xs text-dim tracking-[0.12em]">
      {label}
      <select
        className="border border-idle bg-bg px-[6px] py-[3px] font-mono text-ink text-xs"
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
