import { ChevronDown } from "lucide-react";
import { useEffect, useRef, useState } from "react";

export interface LogDropdownOption {
  value: string;
  label: string;
}

export function LogDropdown({
  value,
  options,
  onChange,
}: {
  value: string;
  options: LogDropdownOption[];
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const current = options.find((option) => option.value === value);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  return (
    <div className={open ? "log-dropdown open" : "log-dropdown"} ref={rootRef}>
      <button
        className="log-dropdown-trigger"
        onClick={() => setOpen((v) => !v)}
        onKeyDown={(event) => {
          if (event.key === "Escape") setOpen(false);
        }}
      >
        <span>{current?.label ?? ""}</span>
        <ChevronDown size={14} />
      </button>
      {open && (
        <div className="log-dropdown-menu">
          {options.map((option) => (
            <button
              key={option.value}
              className={
                option.value === value
                  ? "log-dropdown-item active"
                  : "log-dropdown-item"
              }
              onClick={() => {
                onChange(option.value);
                setOpen(false);
              }}
            >
              {option.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
