import clsx from 'clsx';

export interface DividerProps {
  orientation?: 'horizontal' | 'vertical';
  className?: string;
}

export function Divider({
  orientation = 'horizontal',
  className,
}: DividerProps) {
  return (
    <hr
      aria-orientation={orientation}
      className={clsx(
        'shrink-0 border-0 bg-line',
        orientation === 'vertical' ? 'h-auto w-px self-stretch' : 'h-px w-full',
        className,
      )}
    />
  );
}
