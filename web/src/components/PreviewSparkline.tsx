export interface PreviewSparklineProps {
  /** viewBox coordinate space; the svg itself always renders full-width. */
  width: number;
  height: number;
  line: string;
  area: string;
  title: string;
}

/** Decorative line+area shape for a home preview card. No axes, no labels. */
export function PreviewSparkline({
  width,
  height,
  line,
  area,
  title,
}: PreviewSparklineProps) {
  return (
    <svg
      width="100%"
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className="block"
    >
      <title>{title}</title>
      <path d={area} fill="rgba(247,147,26,0.09)" stroke="none" />
      <path d={line} fill="none" stroke="#f7931a" strokeWidth={1.5} />
    </svg>
  );
}
