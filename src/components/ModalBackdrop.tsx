type ModalBackdropProps = {
  onClick?: () => void;
  fixed?: boolean;
  className?: string;
};

export function ModalBackdrop({
  onClick,
  fixed = false,
  className = "",
}: ModalBackdropProps) {
  return (
    <div
      className={`${fixed ? "fixed" : "absolute"} inset-0 modal-backdrop ${className}`}
      onClick={onClick}
    />
  );
}
