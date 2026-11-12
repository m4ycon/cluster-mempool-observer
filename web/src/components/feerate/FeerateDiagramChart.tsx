import { useFeerateDiagramComparison } from '../../hooks/useFeerateDiagramComparison';
import type { FeerateDiagramWindow } from '../../lib/feerateDiagramChart';
import { FeerateComparisonBar } from './FeerateComparisonBar';
import { FeerateDiagramCurve } from './FeerateDiagramCurve';

export interface FeerateDiagramChartProps {
  blockWindow: FeerateDiagramWindow;
}

export function FeerateDiagramChart({ blockWindow }: FeerateDiagramChartProps) {
  const state = useFeerateDiagramComparison();

  if (state.status === 'loading') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-dim">
        loading feerate diagram...
      </div>
    );
  }

  if (state.status === 'error') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-alert">
        failed to load feerate diagram
      </div>
    );
  }

  if (state.comparison === null) {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-faint">
        no comparison yet
      </div>
    );
  }

  return (
    <div className="flex h-full w-full flex-col gap-2">
      <FeerateComparisonBar comparison={state.comparison} />
      <div className="min-h-0 flex-1">
        <FeerateDiagramCurve
          curves={state.comparison}
          blockWindow={blockWindow}
        />
      </div>
    </div>
  );
}
