import clsx from 'clsx';
import type { ReactNode } from 'react';

export interface VizButtonProps {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
  ariaLabel?: string;
}

/** Segmented-toggle button: orange when active, slate when idle. */
export function VizButton({
  active,
  onClick,
  children,
  ariaLabel,
}: VizButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={ariaLabel}
      className={clsx(
        'mco-reset inline-flex items-center border px-2 py-0.75 text-xs',
        active ? 'border-orange text-orange' : 'border-idle text-slate',
      )}
    >
      {children}
    </button>
  );
}
