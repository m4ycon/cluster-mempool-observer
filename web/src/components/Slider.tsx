export interface SliderProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
}

/** Labeled range input with its current value shown alongside. */
export function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
}: SliderProps) {
  return (
    <label className="flex items-center gap-2 text-xs text-dim tracking-[0.12em]">
      {label}
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
    </label>
  );
}
