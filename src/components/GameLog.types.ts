export type WindowMode = "log" | "crash";
export type Level = "INFO" | "WARN" | "ERROR" | "FATAL" | "DEBUG" | "OTHER";
export type Filter = "ALL" | "WARN" | "ERROR";

export interface LogLine {
  id: number;
  time: string;
  thread: string;
  level: Level;
  body: string;
  raw: string;
}

export interface LogSource {
  id: string;
  label: string;
  kind: string;
  source_path: string;
  modified_ms: number;
}
