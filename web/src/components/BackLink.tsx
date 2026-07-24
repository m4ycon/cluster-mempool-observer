import { Link, type LinkProps } from '@tanstack/react-router';

export interface BackLinkProps {
  to?: LinkProps['to'];
  label?: string;
}

/** Back link; targets the home dashboard by default. */
export function BackLink({ to = '/', label = '← BACK' }: BackLinkProps) {
  return (
    <Link to={to} className="text-xs text-slate hover:text-orange">
      {label}
    </Link>
  );
}
