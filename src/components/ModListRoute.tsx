import type { ReactNode } from "react";
import { enterFromClass, type EnterFrom } from "../utils/viewTransitions";

export function ModListRoute({
  children,
  enterFrom,
}: {
  children: ReactNode;
  enterFrom: EnterFrom;
}) {
  const animationClass = enterFromClass(enterFrom);

  return (
    <div
      className={`${animationClass} flex flex-col flex-1 min-h-0 overflow-hidden`}
      data-view-route="mod-list"
    >
      {children}
    </div>
  );
}
