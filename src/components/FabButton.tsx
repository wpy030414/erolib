import { MdiIcon } from './MdiIcon';

interface FabButtonProps {
  icon: string;
  ariaLabel?: string;
  disabled?: boolean;
  onClick: () => void;
}

export function FabButton({ icon, ariaLabel, disabled, onClick }: FabButtonProps) {
  return (
    <button
      className="fab-button"
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={onClick}
    >
      <MdiIcon path={icon} size={24} />
    </button>
  );
}