export interface StatTileProps {
  label: string;
  value: string;
  unit?: string;
  accent?: boolean;
}

export function StatTile({ label, value, unit, accent }: StatTileProps) {
  return (
    <div className="border border-line bg-bg px-6 py-4">
      <div className="text-xs text-dim tracking-[0.12em]">{label}</div>
      <div className={`mt-1 text-xl ${accent ? 'text-orange' : 'text-ink'}`}>
        {value}
        {unit ? <span className="text-xs text-dim"> {unit}</span> : null}
      </div>
    </div>
  );
}
