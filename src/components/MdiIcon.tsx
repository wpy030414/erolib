interface MdiIconProps {
  path: string;
  size?: number;
  color?: string;
}

export function MdiIcon({ path, size = 24, color }: MdiIconProps) {
  return (
    <svg
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