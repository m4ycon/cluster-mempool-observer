import clsx from 'clsx';
import { type ReactNode, useEffect, useRef, useState } from 'react';

export interface PopoverProps {
  /** Rendered as the trigger; receives the open state to style itself. */
  trigger: (props: { open: boolean; toggle: () => void }) => ReactNode;
  children: ReactNode;
  align?: 'left' | 'right';
}

/**
 * Click-triggered panel anchored to its trigger -- unlike Dialog, it doesn't
 * dim the page, so the visualisation stays visible while filters are open.
 */
export function Popover({ trigger, children, align = 'right' }: PopoverProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative inline-flex">
      {trigger({ open, toggle: () => setOpen((o) => !o) })}
      {open && (
        <div
          className={clsx(
            'absolute top-full z-10 mt-2 flex flex-col gap-3 border border-line bg-bg p-3',
            align === 'right' ? 'right-0' : 'left-0',
          )}
        >
          {children}
        </div>
      )}
    </div>
  );
}
