import type { KeyboardEvent, RefObject } from "react";
import { X } from "lucide-react";
import { C } from "../theme";
import type { LoaderType } from "../types";

export interface CreatingProfileDraft {
  versionId: string;
  versionType?: string;
  loader: LoaderType;
  inputName: string;
}

interface Props {
  creating: CreatingProfileDraft;
  inputRef: RefObject<HTMLInputElement | null>;
  labelForLoader: (loader: string) => string;
  namePlaceholder: string;
  onChangeName: (value: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
}

export function ProfileCreatePanel({
  creating,
  inputRef,
  labelForLoader,
  namePlaceholder,
  onChangeName,
  onCancel,
  onSubmit,
}: Props) {
  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      onSubmit();
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    }
  };

  return (
    <div
      className="glass-panel create-panel"
      style={{
        margin: "4px 8px",
        padding: "10px 12px 12px",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 10,
        }}
      >
        <span style={{ fontSize: 12, fontWeight: 500, color: C.t1 }}>
          {labelForLoader(creating.loader)} {creating.versionId}
        </span>
        <button
          className="icon-btn"
          onClick={onCancel}
          style={{
            marginLeft: "auto",
          }}
        >
          <X size={13} />
        </button>
      </div>
      <input
        className="minimal-input"
        ref={inputRef}
        type="text"
        value={creating.inputName}
        onChange={(e) => onChangeName(e.target.value)}
        placeholder={namePlaceholder}
        style={{
          width: "100%",
        }}
        onFocus={(e) => (e.target.style.borderColor = C.greenBdr)}
        onBlur={(e) => (e.target.style.borderColor = C.b1)}
        onKeyDown={handleKeyDown}
        autoFocus
      />
    </div>
  );
}
