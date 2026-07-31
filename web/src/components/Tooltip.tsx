import clsx from 'clsx';
import { type ReactNode, useId } from 'react';

export interface TooltipProps {
  label: string;
  align?: 'left' | 'right';
  children: ReactNode;
}

export function Tooltip({ label, align = 'left', children }: TooltipProps) {
  const id = useId();

  return (
    <span className="group relative inline-flex" aria-describedby={id}>
      {children}
      <span
        id={id}
        role="tooltip"
        className={clsx(
          'pointer-events-none absolute top-full z-10 mt-2 whitespace-nowrap border border-line bg-bg px-2 py-1 text-dim text-xs tracking-widest opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100',
          align === 'right' ? 'right-0' : 'left-0',
        )}
      >
        {label}
      </span>
    </span>
  );
}
