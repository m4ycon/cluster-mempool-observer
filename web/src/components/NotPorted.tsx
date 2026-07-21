import { Link } from '@tanstack/react-router';

export interface NotPortedProps {
  label: string;
}

export function NotPorted({ label }: NotPortedProps) {
  return (
    <div className="flex flex-1 flex-col px-6 py-5">
      <Link to="/" className="text-xs text-slate hover:text-orange">
        ← BACK
      </Link>
      <div className="mt-6 text-xs text-dim tracking-widest">
        {label} DETAIL · NOT YET PORTED
      </div>
    </div>
  );
}
