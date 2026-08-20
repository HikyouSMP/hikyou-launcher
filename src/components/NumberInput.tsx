import { useEffect, useState } from "react";

interface NumberInputProps {
  value: number;
  onCommit: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  placeholder?: string;
  className?: string;
  style?: React.CSSProperties;
  onFocus?: (event: React.FocusEvent<HTMLInputElement>) => void;
  onBlur?: (event: React.FocusEvent<HTMLInputElement>) => void;
}

export function NumberInput({
  value,
  onCommit,
  min,
  max,
  step,
  placeholder,
  className,
  style,
  onFocus,
  onBlur,
}: NumberInputProps) {
  const [draft, setDraft] = useState(String(value));

  useEffect(() => {
    setDraft(String(value));
  }, [value]);

  const commit = () => {
    const trimmed = draft.trim();
    let next = trimmed === "" ? 0 : Number(trimmed);
    if (!Number.isFinite(next)) next = 0;
    if (min != null) next = Math.max(min, next);
    if (max != null) next = Math.min(max, next);
    setDraft(String(next));
    onCommit(next);
  };

  return (
    <input
      type="number"
      min={min}
      max={max}
      step={step}
      value={draft}
      placeholder={placeholder}
      className={className}
      style={style}
      onChange={(event) => setDraft(event.target.value)}
      onFocus={onFocus}
      onBlur={(event) => {
        commit();
        onBlur?.(event);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.currentTarget.blur();
        }
      }}
    />
  );
}
