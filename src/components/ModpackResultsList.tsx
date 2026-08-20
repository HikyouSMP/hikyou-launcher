import type { MutableRefObject } from "react";
import { Download } from "lucide-react";
import { useTranslation } from "react-i18next";

import { C } from "../theme";
import type { ModSearchResult } from "../types";

export function ModpackResultsList({
  results,
  searching,
  searchValue,
  navItems,
  navIndex,
  hoveredIndex,
  navElemsRef,
  onHoverIndexChange,
  onOpenVersionDialog,
}: {
  results: ModSearchResult[];
  searching: boolean;
  searchValue: string;
  navItems: string[];
  navIndex: number;
  hoveredIndex: number | null;
  navElemsRef: MutableRefObject<Map<string, HTMLElement>>;
  onHoverIndexChange: (index: number | null) => void;
  onOpenVersionDialog: (modpack: ModSearchResult) => void;
}) {
  const { t } = useTranslation();

  const formatDownloads = (count: number) =>
    count >= 1_000_000
      ? `${(count / 1_000_000).toFixed(1)}M`
      : count >= 1_000
        ? `${(count / 1000).toFixed(0)}K`
        : String(count);

  if (searching) {
    return <div className="px-4 py-3 text-t3 text-[12px]">{t("common.searching")}</div>;
  }

  if (!searchValue.trim()) {
    return (
      <div className="py-7.5 px-6 text-center">
        <p className="text-[12px] text-t3 leading-[1.7]">
          {t("modpack.search_hint")}
        </p>
      </div>
    );
  }

  if (results.length === 0) {
    return (
      <div className="py-7.5 px-6 text-center">
        <p className="text-[12px] text-t3">
          {t("modpack.no_match", { query: searchValue })}
        </p>
      </div>
    );
  }

  return (
    <>
      {results.map((modpack, index) => {
        const navKey = `modpack:${index}`;
        const focused = navItems[navIndex] === navKey;
        const hovered = hoveredIndex === index;
        return (
          <div
            key={modpack.project_id}
            ref={(element) => {
              if (element) navElemsRef.current.set(navKey, element);
              else navElemsRef.current.delete(navKey);
            }}
            onMouseEnter={() => onHoverIndexChange(index)}
            onMouseLeave={() => onHoverIndexChange(null)}
            onClick={() => onOpenVersionDialog(modpack)}
            className="flex items-center gap-2.5 px-3 py-1.5 mx-1.5 mb-px rounded-md min-h-11 cursor-pointer"
            style={{
              background: focused ? C.hover : hovered ? C.hoverLight : "transparent",
            }}
          >
            {modpack.icon_url ? (
              <img
                src={modpack.icon_url}
                width={28}
                height={28}
                className="rounded-md shrink-0 object-cover block"
                onError={(event) => {
                  event.currentTarget.style.display = "none";
                }}
              />
            ) : (
              <div
                className="w-7 h-7 rounded-md shrink-0 flex items-center justify-center text-[13px] text-t3 font-semibold"
                style={{ background: "rgba(255,255,255,.055)" }}
              >
                {(modpack.title || "?")[0].toUpperCase()}
              </div>
            )}
            <div className="flex-1 min-w-0">
              <div className="text-[13px] text-t1 whitespace-nowrap overflow-hidden text-ellipsis">
                {modpack.title}
              </div>
              <div className="text-[10px] text-t3 mt-px">
                ↓ {formatDownloads(modpack.downloads)}
              </div>
            </div>
            <button
              onClick={(event) => {
                event.stopPropagation();
                onOpenVersionDialog(modpack);
              }}
              title={t("modpack.select_version_btn")}
              className="w-7 h-7 flex items-center justify-center rounded-md cursor-pointer shrink-0"
              style={{
                background: focused ? C.greenBg : "transparent",
                border: "none",
                color: focused ? C.green : C.t3,
              }}
            >
              <Download size={13} />
            </button>
          </div>
        );
      })}
    </>
  );
}
