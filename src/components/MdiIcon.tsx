interface MdiIconProps {
  path: string;
  size?: number;
  color?: string;
  className?: string;
}

export function MdiIcon({ path, size = 24, color, className }: MdiIconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
      fill={color ?? 'currentColor'}
    >
      <path d={path} />
    </svg>
  );
}