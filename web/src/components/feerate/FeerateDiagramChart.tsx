import { useFeerateDiagram } from '../../hooks/useFeerateDiagram';
import type { FeerateDiagramWindow } from '../../lib/feerateDiagramChart';
import { FeerateDiagramCurve } from './FeerateDiagramCurve';

export interface FeerateDiagramChartProps {
  blockWindow: FeerateDiagramWindow;
}

export function FeerateDiagramChart({ blockWindow }: FeerateDiagramChartProps) {
  const state = useFeerateDiagram();

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

  return (
    <FeerateDiagramCurve diagram={state.diagram} blockWindow={blockWindow} />
  );
}
