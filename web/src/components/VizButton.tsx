import clsx from 'clsx';
import type { ReactNode } from 'react';

export interface VizButtonProps {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
  ariaLabel?: string;
  variant?: 'accent' | 'alert';
}

/** Segmented-toggle button: coloured when active, slate when idle. */
export function VizButton({
  active,
  onClick,
  children,
  ariaLabel,
  variant = 'accent',
}: VizButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={ariaLabel}
      aria-pressed={active}
      className={clsx(
        'mco-reset inline-flex items-center border px-2 py-0.75 text-xs',
        active && variant === 'alert' && 'border-alert text-alert',
        active && variant === 'accent' && 'border-orange text-orange',
        !active && 'border-idle text-slate',
      )}
    >
      {children}
    </button>
  );
}
