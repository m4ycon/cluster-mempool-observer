import clsx from 'clsx';

export interface SliderProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  orientation?: 'horizontal' | 'vertical';
}

/** Labeled range input with its current value shown alongside. */
export function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
  orientation = 'horizontal',
}: SliderProps) {
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
      <span className="flex items-center gap-2">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          className="accent-orange"
        />
        <span className="text-ink">{value}</span>
      </span>
    </label>
  );
}
