import { ArrowLeft } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

export function LibraryShell({
  title,
  subtitle,
  onBack,
  children,
}: {
  title: string;
  subtitle?: string;
  onBack: () => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
      <header data-tauri-drag-region className="flex h-15 shrink-0 items-center gap-3 px-4">
        <button className="icon-btn" onClick={onBack} title={t("common.back")}>
          <ArrowLeft size={15} />
        </button>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[16px] font-medium text-t1">{title}</div>
          {subtitle && <div className="truncate text-[11px] text-t3">{subtitle}</div>}
        </div>
      </header>
      <div className="sb min-h-0 flex-1 overflow-y-auto">{children}</div>
    </div>
  );
}

export function EmptyLibraryState({ children }: { children: ReactNode }) {
  return <div className="px-4 py-10 text-center text-[12px] text-t3">{children}</div>;
}
