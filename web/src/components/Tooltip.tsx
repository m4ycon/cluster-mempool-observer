import clsx from 'clsx';
import { type ReactNode, useId } from 'react';
import { TooltipBubble } from './TooltipBubble';

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
      <TooltipBubble
        id={id}
        className={clsx(
          'absolute top-full z-10 mt-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100',
          align === 'right' ? 'right-0' : 'left-0',
        )}
      >
        {label}
      </TooltipBubble>
    </span>
  );
}
