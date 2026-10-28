import type { Snippet } from "svelte";
import type { ModSearchResult } from "../types";

export interface MLEntry {
  id: string;
  name: string;
  icon?: string | null;
  enabled: boolean;
  subtitle?: string;
  badges?: Snippet;
}

export interface ModListViewProps {
  onClose: () => void;
  embedded?: boolean;
  searchPlaceholder: string;
  headerRight?: Snippet;
  entries: MLEntry[];
  loading: boolean;
  emptyNode?: Snippet;
  errorNode?: Snippet;
  onEntryClick?: (id: string) => void;
  onEntryDelete: (id: string) => void;
  loader: string;
  mcVersion: string;
  searchState: (projectId: string) => "idle" | "loading" | "done";
  onInstall: (mod: ModSearchResult) => Promise<void>;
  onSearchResultDelete?: (mod: ModSearchResult) => void;
  renderSearchMeta?: Snippet<[ModSearchResult]>;
  renderEntryBadges?: Snippet<[MLEntry]>;
}
