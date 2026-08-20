import type { CSSProperties } from "react";
import { C } from "../theme";
import type { Profile } from "../types";
import { loaderBadgeClass, loaderDisplayLabel } from "../utils/profileDisplay";
import { ProfileProgressBar } from "./ProfileProgressBar";

type OptionsCopyPulse = { kind: "pick" | "drop"; x: number; y: number } | null;

export function RunningProfileRow({
  profile,
  statusText,
  smartStatus,
  progressPercent,
  onContextMenu,
  optionsCopySource,
  optionsCopyPulse,
  onMiddlePick,
  onMiddleDrop,
}: {
  profile: Profile;
  statusText: string;
  smartStatus?: string | null;
  progressPercent?: number | null;
  onContextMenu: (event: React.MouseEvent) => void;
  optionsCopySource?: boolean;
  optionsCopyPulse?: OptionsCopyPulse;
  onMiddlePick?: (profile: Profile, event: React.MouseEvent<HTMLElement>) => void;
  onMiddleDrop?: (profile: Profile, event: React.MouseEvent<HTMLElement>) => void;
}) {
  return (
    <div className="contents">
      <div
        className="pi running-item block mx-1.5 mb-px p-0 rounded-md overflow-hidden"
        data-options-copy={optionsCopySource ? "source" : undefined}
        data-options-copy-pulse={optionsCopyPulse?.kind ?? undefined}
        style={{
          "--ripple-x": `${optionsCopyPulse?.x ?? 50}%`,
          "--ripple-y": `${optionsCopyPulse?.y ?? 50}%`,
          background: optionsCopySource
            ? "rgba(255,255,255,.075)"
            : "rgba(61,168,92,.055)",
          backdropFilter: "blur(16px) saturate(145%)",
          WebkitBackdropFilter: "blur(16px) saturate(145%)",
        } as CSSProperties}
        onContextMenu={onContextMenu}
        onMouseDown={(event) => {
          if (event.button === 1) {
            event.preventDefault();
            onMiddlePick?.(profile, event);
          }
        }}
        onMouseUp={(event) => {
          if (event.button === 1) {
            event.preventDefault();
            onMiddleDrop?.(profile, event);
          }
        }}
      >
        <div
          className="relative flex items-center gap-2 px-3 py-1.75"
          style={{
            borderRadius: C.r,
            background: "rgba(61,168,92,.07)",
            paddingBottom: 7,
          }}
        >
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-1.25 flex-wrap">
              <p className="text-[13px] font-normal text-t1 overflow-hidden text-ellipsis whitespace-nowrap shrink">
                {profile.name}
              </p>
              <span
                className={
                  loaderBadgeClass(profile.loader) +
                  " text-[10px] font-medium px-1.25 py-px rounded-md whitespace-nowrap shrink-0 tracking-[0.02em]"
                }
              >
                {loaderDisplayLabel(profile.loader)}
              </span>
              <span
                className="text-[9px] font-medium px-1.25 py-px rounded-md text-t2 whitespace-nowrap shrink-0 font-mono"
                style={{ background: "rgba(255,255,255,.06)" }}
              >
                {profile.mcVersion}
              </span>
              {smartStatus && (
                <span className="smart-status-chip">{smartStatus}</span>
              )}
            </div>
          </div>
          <div className="flex items-center gap-1.5 shrink-0">
            <span className="text-green text-[11px] font-semibold">
              {statusText}
            </span>
          </div>
          {progressPercent !== null && progressPercent !== undefined && (
            <ProfileProgressBar percent={progressPercent} />
          )}
          {progressPercent === undefined && <ProfileProgressBar percent={null} />}
        </div>
      </div>
    </div>
  );
}

export function ProfileRow({
  profile,
  focused,
  hovered,
  ctrlHeld,
  ctrlIndex,
  optionsCopySource,
  optionsCopyPulse,
  smartStatus,
  navRef,
  onHoverChange,
  onActivate,
  onDelete,
  onContextMenu,
  onMiddlePick,
  onMiddleDrop,
}: {
  profile: Profile;
  focused: boolean;
  hovered: boolean;
  ctrlHeld: boolean;
  ctrlIndex: number;
  optionsCopySource: boolean;
  optionsCopyPulse: OptionsCopyPulse;
  smartStatus?: string | null;
  navRef: (element: HTMLElement | null) => void;
  onHoverChange: (hovered: boolean) => void;
  onActivate: () => void;
  onDelete: () => void;
  onContextMenu: (event: React.MouseEvent) => void;
  onMiddlePick: (profile: Profile, event: React.MouseEvent<HTMLElement>) => void;
  onMiddleDrop: (profile: Profile, event: React.MouseEvent<HTMLElement>) => void;
}) {
  const smart = profile.kind === "smart";
  return (
    <div style={{ display: "contents" }}>
      <div
        role="button"
        tabIndex={0}
        ref={navRef}
        onClick={onActivate}
        onContextMenu={onContextMenu}
        onMouseDown={(event) => {
          if (event.button === 1) {
            event.preventDefault();
            onMiddlePick(profile, event);
          }
        }}
        onMouseUp={(event) => {
          if (event.button === 1) {
            event.preventDefault();
            onMiddleDrop(profile, event);
          }
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") onActivate();
        }}
        className="pi"
        data-options-copy={optionsCopySource ? "source" : undefined}
        data-options-copy-pulse={optionsCopyPulse?.kind ?? undefined}
        style={{
          "--ripple-x": `${optionsCopyPulse?.x ?? 50}%`,
          "--ripple-y": `${optionsCopyPulse?.y ?? 50}%`,
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "7px 12px",
          margin: "0 6px 1px",
          borderRadius: C.r,
          cursor: "pointer",
          background: focused
            ? "rgba(255,255,255,.085)"
            : optionsCopySource
              ? "rgba(255,255,255,.075)"
              : hovered
              ? "rgba(255,255,255,.065)"
              : "transparent",
          transition: "none",
          outline: "none",
          position: "relative",
        } as CSSProperties}
        onMouseEnter={() => onHoverChange(true)}
        onMouseLeave={() => onHoverChange(false)}
      >
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              flexWrap: "wrap",
            }}
          >
            <p
              style={{
                fontSize: 13,
                fontWeight: 400,
                color: C.t1,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                flexShrink: 1,
              }}
            >
              {profile.name}
            </p>
            <span
              className={loaderBadgeClass(profile.loader)}
              style={{
                fontSize: 10,
                fontWeight: 500,
                padding: "1px 5px",
                borderRadius: C.r,
                whiteSpace: "nowrap",
                flexShrink: 0,
                letterSpacing: "0.02em",
              }}
            >
              {loaderDisplayLabel(profile.loader)}
            </span>
            <span
              style={{
                fontSize: 10,
                fontWeight: 400,
                padding: "1px 6px",
                borderRadius: C.r,
                background: "rgba(255,255,255,.055)",
                color: C.t2,
                whiteSpace: "nowrap",
                flexShrink: 0,
                fontFamily: "'JetBrains Mono','SF Mono',monospace",
              }}
            >
              {profile.mcVersion}
            </span>
            {smartStatus && (
              <span className="smart-status-chip">{smartStatus}</span>
            )}
          </div>
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 3,
            flexShrink: 0,
          }}
        >
          {!smart && (
            <button
              className="del"
              onClick={(event) => {
                event.stopPropagation();
                onDelete();
              }}
              style={{
                width: 20,
                height: 20,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                borderRadius: 5,
                background: "transparent",
                border: "none",
                color: C.t3,
                fontSize: 14,
                cursor: "pointer",
                lineHeight: 1,
                transition: "all .12s",
              }}
              onMouseEnter={(event) => {
                event.currentTarget.style.color = C.danger;
                event.currentTarget.style.background = C.dangerBg;
              }}
              onMouseLeave={(event) => {
                event.currentTarget.style.color = C.t3;
                event.currentTarget.style.background = "transparent";
              }}
            >
              ×
            </button>
          )}
          {ctrlHeld && ctrlIndex >= 0 && ctrlIndex < 10 && (
            <span
              style={{
                position: "absolute",
                right: 10,
                top: "50%",
                width: 20,
                height: 20,
                borderRadius: 5,
                background: focused
                  ? "rgba(255,255,255,.075)"
                  : "rgba(255,255,255,.06)",
                color: focused ? C.t2 : C.t3,
                fontSize: 11,
                fontFamily: "'JetBrains Mono','SF Mono',monospace",
                fontWeight: 400,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                zIndex: 2,
                pointerEvents: "none",
                animation: "ctrlBadgeIn .15s cubic-bezier(.16,1,.3,1) forwards",
              }}
            >
              {ctrlIndex === 9 ? "0" : String(ctrlIndex + 1)}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
