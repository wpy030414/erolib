import { MdiIcon } from './MdiIcon';

interface FabButtonProps {
  icon: string;
  ariaLabel?: string;
  disabled?: boolean;
  onClick: () => void;
  style?: React.CSSProperties;
}

export function FabButton({ icon, ariaLabel, disabled, onClick, style }: FabButtonProps) {
  return (
    <button
      className="fab-button"
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={onClick}
      style={style}
    >
      <MdiIcon path={icon} size={24} />
    </button>
  );
}