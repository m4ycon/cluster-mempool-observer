import clsx from 'clsx';

export interface VizButtonProps {
  active: boolean;
  onClick: () => void;
  children: string;
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
        'mco-reset border px-2 py-[3px] text-xs',
        active ? 'border-orange text-orange' : 'border-idle text-slate',
      )}
    >
      {children}
    </button>
  );
}
