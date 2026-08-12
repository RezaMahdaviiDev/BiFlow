import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { replaceJsonVersion } from "./sync-version.mjs";

describe("version synchronization", () => {
  it("changes only the version value and preserves JSON formatting", () => {
    const source = `{
  "name": "biflow",
  "version": "1.0.0",
  "depends": ["one", "two"]
}
`;

    assert.equal(
      replaceJsonVersion(source, "1.1.0"),
      source.replace('"version": "1.0.0"', '"version": "1.1.0"'),
    );
  });
});
