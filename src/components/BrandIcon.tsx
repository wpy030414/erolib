interface BrandIconProps {
  path: string;
  viewBox?: string;
  size?: number;
  fillRule?: 'nonzero' | 'evenodd';
  brand?: boolean;
}

const SCALE = 0.75;

export function BrandIcon({
  path,
  viewBox = '0 0 24 24',
  size = 24,
  fillRule,
  brand = false,
}: BrandIconProps) {
  const parts = viewBox.split(/\s+/).map(Number);
  const w = parts[2] ?? 24;
  const h = parts[3] ?? 24;
  const tx = (w - w * SCALE) / 2;
  const ty = (h - h * SCALE) / 2;
  const transform = brand ? `translate(${tx}, ${ty}) scale(${SCALE})` : undefined;

  return (
    <svg
      width={size}
      height={size}
      viewBox={viewBox}
      aria-hidden="true"
      focusable="false"
      fill="currentColor"
    >
      <g transform={transform}>
        <path d={path} fillRule={fillRule} clipRule={fillRule} />
      </g>
    </svg>
  );
}