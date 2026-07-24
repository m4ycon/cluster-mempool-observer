import { BackLink } from './BackLink';

export interface NotPortedProps {
  label: string;
}

export function NotPorted({ label }: NotPortedProps) {
  return (
    <div className="flex flex-1 flex-col px-6 py-5">
      <BackLink />
      <div className="mt-6 text-xs text-dim tracking-widest">
        {label} DETAIL · NOT YET PORTED
      </div>
    </div>
  );
}
