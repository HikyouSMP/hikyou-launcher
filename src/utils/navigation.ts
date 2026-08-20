import type { LoaderType } from "../types";

export type MainEnterTarget =
  | { kind: "profile"; profileId: string }
  | { kind: "create"; versionId: string; loader: LoaderType }
  | null;

function parseNavKey(key: string | undefined | null): MainEnterTarget {
  if (!key) return null;
  if (key.startsWith("p:")) {
    return { kind: "profile", profileId: key.slice(2) };
  }
  if (key.startsWith("c:")) {
    const [, versionId, loader] = key.split(":");
    if (!versionId || !loader) return null;
    return { kind: "create", versionId, loader: loader as LoaderType };
  }
  return null;
}

export function resolveMainEnterTarget({
  navItems,
  navIndex,
  hoverProfileId,
  hoverVersionKey,
}: {
  navItems: string[];
  navIndex: number;
  hoverProfileId: string | null;
  hoverVersionKey: string | null;
}): MainEnterTarget {
  if (hoverProfileId && navItems.includes(`p:${hoverProfileId}`)) {
    return { kind: "profile", profileId: hoverProfileId };
  }

  const hoveredCreate = navItems.includes(hoverVersionKey ?? "")
    ? parseNavKey(hoverVersionKey)
    : null;
  if (hoveredCreate) return hoveredCreate;

  const indexed = parseNavKey(navItems[navIndex]);
  if (indexed) return indexed;

  return parseNavKey(navItems[0]);
}
