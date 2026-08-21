import clsx from 'clsx';
import type { CSSProperties, ReactNode } from 'react';

export interface TooltipBubbleProps {
  id?: string;
  className?: string;
  style?: CSSProperties;
  children: ReactNode;
}

export function TooltipBubble({
  id,
  className,
  style,
  children,
}: TooltipBubbleProps) {
  return (
    <span
      id={id}
      role="tooltip"
      style={style}
      className={clsx(
        'pointer-events-none whitespace-nowrap border border-line bg-bg px-2 py-1 text-dim text-xs tracking-widest',
        className,
      )}
    >
      {children}
    </span>
  );
}
