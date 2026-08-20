import { invoke } from "@tauri-apps/api/core";
import type { KeyboardEvent } from "react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { C } from "../theme";

function codeToDisplay(code: string): string | null {
  if (code === "Space") return "Space";
  if (code === "Enter") return "Enter";
  if (code === "Tab") return "Tab";
  if (code === "Backspace") return "Backspace";
  if (code.startsWith("Key") && code.length === 4) return code.slice(3);
  if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  return null;
}

export function ShortcutCapture({
  value,
  isMac,
  onConfirm,
}: {
  value: string;
  isMac: boolean;
  onConfirm: (shortcut: string) => void;
}) {
  const { t } = useTranslation();
  const [recording, setRecording] = useState(false);
  const confirmedRef = useRef(false);
  const parts = value.split("+");

  const startRecording = () => {
    confirmedRef.current = false;
    invoke("suspend_shortcut").catch(console.error);
    setRecording(true);
  };

  const handleBlur = () => {
    if (!recording) return;
    if (!confirmedRef.current) {
      invoke("register_shortcut", { shortcutStr: value }).catch(console.error);
    }
    confirmedRef.current = false;
    setRecording(false);
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return;

    const mods: string[] = [];
    if (event.ctrlKey) mods.push("Ctrl");
    if (event.altKey) mods.push(isMac ? "Option" : "Alt");
    if (event.shiftKey) mods.push("Shift");
    if (event.metaKey) mods.push("Cmd");
    if (mods.length === 0) return;

    const key = codeToDisplay(event.code);
    if (!key) return;

    confirmedRef.current = true;
    onConfirm([...mods, key].join("+"));
    setRecording(false);
  };

  return (
    <div
      tabIndex={0}
      onClick={startRecording}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      className="flex items-center gap-1 px-2 py-1.25 rounded-md cursor-pointer outline-none"
      style={{
        border: `1px solid ${recording ? C.greenBdr : C.b1}`,
        background: recording ? C.greenBg : "transparent",
        minWidth: 100,
      }}
    >
      {recording ? (
        <span className="text-[11px] text-t3">
          {t("settings.shortcut_recording")}
        </span>
      ) : (
        parts.map((part, index) => (
          <span
            key={index}
            className="text-[10px] px-1.5 py-0.5 rounded-[3px] text-t2 font-mono"
            style={{ background: C.hover }}
          >
            {part}
          </span>
        ))
      )}
    </div>
  );
}
