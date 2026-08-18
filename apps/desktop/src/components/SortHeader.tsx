import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";
import type { SortState } from "../lib/tableSort";

/** Column header that toggles asc/desc sorting for its column. */
export function SortHeader<K extends string>({
  label,
  sortKey,
  state,
  onToggle,
}: {
  label: string;
  sortKey: K;
  state: SortState<K>;
  onToggle: (key: K) => void;
}) {
  const active = state.key === sortKey;
  const Icon = active ? (state.dir === "asc" ? ArrowUp : ArrowDown) : ArrowUpDown;
  return (
    <th
      className="px-3 py-2 text-start font-medium"
      aria-sort={
        active ? (state.dir === "asc" ? "ascending" : "descending") : undefined
      }
    >
      <button
        type="button"
        onClick={() => onToggle(sortKey)}
        className={`inline-flex items-center gap-1 hover:text-ink ${
          active ? "text-ink" : ""
        }`}
      >
        {label}
        <Icon size={14} aria-hidden className={active ? "" : "opacity-40"} />
      </button>
    </th>
  );
}
