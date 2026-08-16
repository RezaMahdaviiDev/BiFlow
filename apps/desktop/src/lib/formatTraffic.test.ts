import { describe, expect, it } from "vitest";
import { formatTrafficBytes } from "./formatTraffic";

describe("formatTrafficBytes", () => {
  it("keeps exact bytes below one kibibyte", () => {
    expect(formatTrafficBytes(0)).toBe("0 B");
    expect(formatTrafficBytes(512)).toBe("512 B");
    expect(formatTrafficBytes(1023)).toBe("1023 B");
  });

  it("uses two decimals for mid-range totals and one for large values", () => {
    expect(formatTrafficBytes(1024)).toBe("1.00 KiB");
    expect(formatTrafficBytes(1_048_576)).toBe("1.00 MiB");
    expect(formatTrafficBytes(12_345_678)).toBe("11.77 MiB");
    expect(formatTrafficBytes(10_737_418_240)).toBe("10.00 GiB");
    expect(formatTrafficBytes(1_099_511_627_776)).toBe("1.00 TiB");
    expect(formatTrafficBytes(123_480_309_760)).toBe("115.0 GiB");
  });
});
