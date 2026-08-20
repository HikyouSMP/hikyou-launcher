import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import i18n from "../i18n";
import type { CrashAnalysis, ParsedCrash } from "../types";

type Args = {
  crashToastProfileId: string | null;
  crashAnalyses: Record<string, CrashAnalysis>;
  setCrashAnalyses: React.Dispatch<
    React.SetStateAction<Record<string, CrashAnalysis>>
  >;
};

export function useCrashToastParser({
  crashToastProfileId,
  crashAnalyses,
  setCrashAnalyses,
}: Args) {
  useEffect(() => {
    const toast = crashToastProfileId
      ? crashAnalyses[crashToastProfileId]
      : null;
    if (!toast?.lines?.length) return;
    invoke<ParsedCrash>("parse_crash_log", {
      logLines: toast.lines,
      lang: i18n.language?.startsWith("en") ? "en" : "ja",
    })
      .then((parsed) => {
        setCrashAnalyses((prev) => ({
          ...prev,
          [toast.profile_id]: { ...toast, parsed },
        }));
      })
      .catch(console.error);
  }, [crashToastProfileId, crashAnalyses, setCrashAnalyses]);
}
