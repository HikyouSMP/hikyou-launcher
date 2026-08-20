import { useState } from "react";
import type { DownloadProgress, GamePhase } from "../types";

export function useLaunchState() {
  const [profileRunStates, setProfileRunStates] = useState<
    Record<string, { phase: GamePhase; dlProgress: DownloadProgress | null }>
  >({});
  const [gameError, setGameError] = useState<string | undefined>();
  const [profileLogs, setProfileLogs] = useState<Record<string, string[]>>({});
  const [profileCtxMenu, setProfileCtxMenu] = useState<{
    profileId: string;
    x: number;
    y: number;
  } | null>(null);

  return {
    profileRunStates,
    setProfileRunStates,
    gameError,
    setGameError,
    profileLogs,
    setProfileLogs,
    profileCtxMenu,
    setProfileCtxMenu,
  };
}
