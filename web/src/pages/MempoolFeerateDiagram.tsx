import { useState } from 'react';
import { BackLink } from '../components/BackLink';
import { FeerateDiagramChart } from '../components/feerate/FeerateDiagramChart';
import { FeerateWindowFilter } from '../components/feerate/FeerateWindowFilter';
import type { FeerateDiagramWindow } from '../lib/feerateDiagramChart';

export function MempoolFeerateDiagram() {
  // 'all' shows the whole mempool on open -- the user chose that default explicitly.
  const [blockWindow, setBlockWindow] = useState<FeerateDiagramWindow>('all');

  return (
    <div
      className="flex flex-1 flex-col bg-bg"
      data-screen-label="Mempool feerate diagram"
    >
      <div className="flex items-baseline gap-4 border-line border-b px-6 py-3">
        <BackLink />
        <span className="text-xs text-ink tracking-widest">
          MEMPOOL FEERATE DIAGRAM
        </span>
      </div>

      <FeerateWindowFilter value={blockWindow} onChange={setBlockWindow} />

      <div className="flex-1 px-6 py-5">
        <div className="h-140">
          <FeerateDiagramChart blockWindow={blockWindow} />
        </div>
      </div>
    </div>
  );
}
