import { modLifecycleEvents } from "./modLifecycle";

type Operation = "get_profile_mod_snapshot" | "set_profile_mod_choice" | "install_modrinth_mod" | "get_mod_decisions" | "answer_mod_decision" | "get_mod_template" | "save_mod_template" | "get_plus_play_setup" | "confirm_plus_play_setup" | typeof modLifecycleEvents[number];
export type ModFailureCode = "stale" | "profile_mismatch" | "missing" | "local_object_missing" | "state_unreadable" | "state_invalid" | "busy" | "permission_denied" | "provider_unavailable" | "resolution_limit" | "resolution_unsatisfiable" | "unknown";
export interface ModLifecycleFailure { operation: Operation; code: ModFailureCode; kind: "ipc" | "error" | "unknown" }

// Project known causes, not arbitrary text: paths, URLs, credentials and raw logs never become UI content.
export function modLifecycleFailure(reason: unknown, operation: Operation): ModLifecycleFailure {
  const kind = typeof reason === "string" ? "ipc" : reason instanceof Error ? "error" : "unknown";
  const message = (typeof reason === "string" ? reason : reason instanceof Error ? reason.message : "").slice(0, 8192).replace(/^Error:\s*/, "");
  let code: ModFailureCode = "unknown";
  if (/^(template_preview_changed|template_changed|plus_setup_changed|Mod (choices|inputs|decision( inputs)?|state) changed|Active Mod configuration changed|Mod alternative changed)/i.test(message)) code = "stale";
  else if (/^(profile_mismatch|Profile identity mismatch|Mod decision profile or identity mismatch)/i.test(message)) code = "profile_mismatch";
  else if (/^Exact local Mod object is unavailable/i.test(message)) code = "local_object_missing";
  else if (/^(profile not found|Mod decision not found|Mod key is not present|Mod wishes are missing|Mod decision is no longer current)/i.test(message)) code = "missing";
  else if (/^(Cannot read Mod|Cannot open Mod|Mod state is unreadable)/i.test(message)) code = "state_unreadable";
  else if (/^(Invalid Mod (decision|snapshot)|Unsupported Mod decision|Mod (decision|snapshot).*(exceeds|mismatch)|Mod decision evidence is missing)/i.test(message)) code = "state_invalid";
  else if (/^(Profile is (busy|running)|Mod (preparation|maintenance) is (busy|running)|Another Mod operation)/i.test(message)) code = "busy";
  else if (/^(Permission denied|Access is denied|access denied)/i.test(message)) code = "permission_denied";
  else if (/^(provider_unavailable|mod_catalog_(unavailable|http_failed)|template_catalog_timeout|plus_setup_catalog_timeout|Mod provider unavailable|Modrinth unavailable)/i.test(message)) code = "provider_unavailable";
  else if (/^mod_resolution_limit:/.test(message)) code = "resolution_limit";
  else if (/^mod_resolution_unsatisfiable:/.test(message)) code = "resolution_unsatisfiable";
  return { operation, code, kind };
}
