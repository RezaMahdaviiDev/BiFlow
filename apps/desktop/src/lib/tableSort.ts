export type SortDir = "asc" | "desc";

export interface SortState<K extends string> {
  key: K;
  dir: SortDir;
}

/**
 * Clicking the active column flips its direction; clicking another column
 * activates it with the given default direction.
 */
export function toggleSort<K extends string>(
  state: SortState<K>,
  key: K,
  defaultDir: SortDir = "asc",
): SortState<K> {
  if (state.key === key) {
    return { key, dir: state.dir === "asc" ? "desc" : "asc" };
  }
  return { key, dir: defaultDir };
}

/** Stable sort of rows by the accessor the current sort state selects. */
export function sortRows<T, K extends string>(
  rows: readonly T[],
  state: SortState<K>,
  accessors: Record<K, (row: T) => string | number>,
): T[] {
  const accessor = accessors[state.key];
  const factor = state.dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const left = accessor(a);
    const right = accessor(b);
    const compared =
      typeof left === "number" && typeof right === "number"
        ? left - right
        : String(left).localeCompare(String(right));
    return compared * factor;
  });
}
