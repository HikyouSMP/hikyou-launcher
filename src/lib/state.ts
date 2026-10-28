export type Update<T> = T | ((previous: T) => T);
export type Setter<T> = (update: Update<T>) => void;
export interface Ref<T> { current: T }

export function resolveUpdate<T>(previous: T, update: Update<T>): T {
  return typeof update === "function"
    ? (update as (previous: T) => T)(previous)
    : update;
}
