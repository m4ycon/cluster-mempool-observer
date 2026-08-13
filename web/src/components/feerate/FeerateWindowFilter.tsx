import type { FeerateDiagramWindow } from '../../lib/feerateDiagramChart';
import { VizButton } from '../VizButton';

export interface FeerateWindowFilterProps {
  value: FeerateDiagramWindow;
  onChange: (value: FeerateDiagramWindow) => void;
}

const OPTIONS: readonly { value: FeerateDiagramWindow; label: string }[] = [
  { value: 1, label: '1 block' },
  { value: 2, label: '2 blocks' },
  { value: 3, label: '3 blocks' },
  { value: 'all', label: 'all' },
];

/** Block-window toggle row. */
export function FeerateWindowFilter({
  value,
  onChange,
}: FeerateWindowFilterProps) {
  return (
    <div className="flex items-center gap-2 border-line border-b px-6 py-2">
      <span className="text-xs text-dim tracking-[0.12em]">WINDOW</span>
      <div className="flex gap-1">
        {OPTIONS.map((opt) => (
          <VizButton
            key={opt.value}
            active={opt.value === value}
            onClick={() => onChange(opt.value)}
          >
            {opt.label}
          </VizButton>
        ))}
      </div>
    </div>
  );
}
