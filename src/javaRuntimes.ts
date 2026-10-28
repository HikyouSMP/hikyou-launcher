export interface JavaInstallation {
  directory: string; executable: string; major: number; javaVersion: string; vendor: string;
  selected: boolean; legacy: boolean; error: string | null;
}
export interface JavaUpdateCheck {
  major: number; lastCheckedAt: number | null; lastAttemptAt: number | null;
  nextCheckAt: number | null; lastError: string | null; checking: boolean;
  deferred: boolean; cleanupError: string | null;
}
export interface JavaInventory { installations: JavaInstallation[]; checks: JavaUpdateCheck[] }
export interface JavaManualCheck { status: JavaInventory; checked: number; failures: string[] }
export interface JavaRuntimeChoice { path: string; major: number; javaVersion: string; vendor: string }

export const managedJavaName = (runtime: JavaInstallation) => runtime.directory.startsWith("zulu-") && runtime.vendor.toLowerCase().includes("azul") ? "Azul Zulu" : runtime.vendor;

export const javaRuntimeState = (runtime: JavaInstallation) => runtime.error ? "invalid" : runtime.legacy ? "excluded" : runtime.selected ? "default" : "cleanup";
