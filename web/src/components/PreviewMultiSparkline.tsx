export interface PreviewMultiSparklineProps {
  /** viewBox coordinate space; the svg itself always renders full-width. */
  width: number;
  height: number;
  lines: { path: string; color: string }[];
  title: string;
}

/** Decorative multi-line shape for a home preview card. No axes, no labels, no fill. */
export function PreviewMultiSparkline({
  width,
  height,
  lines,
  title,
}: PreviewMultiSparklineProps) {
  return (
    <svg
      width="100%"
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className="block"
    >
      <title>{title}</title>
      {lines.map(({ path, color }) => (
        <path
          key={color}
          d={path}
          fill="none"
          stroke={color}
          strokeWidth={1.5}
        />
      ))}
    </svg>
  );
}
