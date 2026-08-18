import { describe, expect, it } from "vitest";
import { sortRows, toggleSort } from "./tableSort";

type Row = { name: string; count: number };
const rows: Row[] = [
  { name: "beta", count: 2 },
  { name: "alpha", count: 5 },
  { name: "gamma", count: 1 },
];
const accessors = {
  name: (row: Row) => row.name,
  count: (row: Row) => row.count,
};

describe("toggleSort", () => {
  it("flips direction when the same column is clicked", () => {
    const first = toggleSort({ key: "count", dir: "desc" }, "count");
    expect(first).toEqual({ key: "count", dir: "asc" });
    expect(toggleSort(first, "count")).toEqual({ key: "count", dir: "desc" });
  });

  it("switches column with the default direction", () => {
    expect(toggleSort({ key: "count", dir: "desc" }, "name")).toEqual({
      key: "name",
      dir: "asc",
    });
    expect(toggleSort({ key: "count", dir: "desc" }, "name", "desc")).toEqual({
      key: "name",
      dir: "desc",
    });
  });
});

describe("sortRows", () => {
  it("sorts numbers numerically in both directions", () => {
    expect(
      sortRows(rows, { key: "count", dir: "asc" }, accessors).map(
        (row) => row.count,
      ),
    ).toEqual([1, 2, 5]);
    expect(
      sortRows(rows, { key: "count", dir: "desc" }, accessors).map(
        (row) => row.count,
      ),
    ).toEqual([5, 2, 1]);
  });

  it("sorts strings with locale compare and leaves the input untouched", () => {
    const sorted = sortRows(rows, { key: "name", dir: "asc" }, accessors);
    expect(sorted.map((row) => row.name)).toEqual(["alpha", "beta", "gamma"]);
    expect(rows[0]?.name).toBe("beta");
  });
});
