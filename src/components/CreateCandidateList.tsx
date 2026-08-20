import type { MutableRefObject, RefObject } from "react";

import { C } from "../theme";
import type { LoaderType, VersionEntry } from "../types";
import { isLoaderCompatible } from "../utils/versionCompatibility";

export interface CreatingProfileDraft {
  versionId: string;
  versionType?: string;
  loader: LoaderType;
  inputName: string;
}

export function CreateCandidateList({
  versions,
  loaders,
  navItems,
  navIndex,
  hoverKey,
  navElemsRef,
  createInputRef,
  labelForLoader,
  onHoverKeyChange,
  onCreateDraft,
}: {
  versions: VersionEntry[];
  loaders: LoaderType[];
  navItems: string[];
  navIndex: number;
  hoverKey: string | null;
  navElemsRef: MutableRefObject<Map<string, HTMLElement>>;
  createInputRef: RefObject<HTMLInputElement | null>;
  labelForLoader: (loader: LoaderType | string) => string;
  onHoverKeyChange: (key: string | null) => void;
  onCreateDraft: (draft: CreatingProfileDraft) => void;
}) {
  return (
    <>
      {versions.flatMap((version) =>
        loaders.map((loader) => {
          if (!isLoaderCompatible(loader, version.id)) return null;
          const navKey = `c:${version.id}:${loader}`;
          const focused = navItems[navIndex] === navKey;
          const label = `${labelForLoader(loader)} ${version.id}`;
          const startCreate = () => {
            onCreateDraft({
              versionId: version.id,
              versionType: version.type,
              loader,
              inputName: "",
            });
            setTimeout(() => createInputRef.current?.focus(), 30);
          };

          return (
            <div
              key={`c-${version.id}-${loader}`}
              role="button"
              tabIndex={0}
              ref={(element) => {
                if (element) navElemsRef.current.set(navKey, element);
                else navElemsRef.current.delete(navKey);
              }}
              onClick={startCreate}
              onKeyDown={(event) => {
                if (event.key === "Enter") startCreate();
              }}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "9px 12px",
                margin: "0 6px 1px",
                borderRadius: C.r,
                cursor: "pointer",
                border: "1px solid transparent",
                background: focused || hoverKey === navKey ? C.hover : "transparent",
                transition: "background .08s",
                outline: "none",
              }}
              onMouseEnter={() => onHoverKeyChange(navKey)}
              onMouseLeave={() => onHoverKeyChange(null)}
            >
              <div style={{ flex: 1, minWidth: 0 }}>
                <p
                  style={{
                    fontSize: 13,
                    fontWeight: 400,
                    color: C.t1,
                    fontFamily: "'Inter',system-ui,sans-serif",
                  }}
                >
                  {label}
                </p>
              </div>
            </div>
          );
        }),
      )}
    </>
  );
}
