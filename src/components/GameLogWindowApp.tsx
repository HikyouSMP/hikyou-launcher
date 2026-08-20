import React, { Suspense, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

import { ErrorBoundary } from "./ErrorBoundary";
import { LogInspectorLoading } from "./LogInspectorLoading";
import i18n from "../i18n";

const GameLog = React.lazy(() =>
  import("./GameLog").then((m) => ({ default: m.GameLog })),
);

export function GameLogWindowApp() {
  useEffect(() => {
    const unlisten = listen<string>("log://language-changed", (event) => {
      const locale = event.payload;
      if (locale) i18n.changeLanguage(locale).catch(console.error);
    });
    return () => {
      unlisten.then((dispose) => dispose()).catch(console.error);
    };
  }, []);

  return (
    <ErrorBoundary>
      <Suspense fallback={<LogInspectorLoading />}>
        <GameLog mode="log" />
      </Suspense>
    </ErrorBoundary>
  );
}
