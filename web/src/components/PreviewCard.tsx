import type { ReactNode } from 'react';

export interface PreviewCardProps {
  title: string;
  caption: string;
  onClick: () => void;
  children: ReactNode;
}

export function PreviewCard({
  title,
  caption,
  onClick,
  children,
}: PreviewCardProps) {
  return (
    <button
      type="button"
      className="mco-reset group block w-full border border-line bg-bg"
      onClick={onClick}
    >
      <div className="px-6 pt-4 pb-5 group-hover:bg-card-hover">
        <div className="mb-4 text-xs text-muted tracking-widest">{title}</div>
        <div className="h-36 opacity-50">{children}</div>
        <div className="mt-4 flex items-baseline justify-between">
          <span className="text-xs text-dim">{caption}</span>
          <span className="border-dash border-b border-dashed text-xs text-orange">
            OPEN →
          </span>
        </div>
      </div>
    </button>
  );
}
