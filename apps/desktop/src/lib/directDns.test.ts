import { describe, expect, it } from "vitest";
import {
  DIRECT_DNS_PRESET_SERVERS,
  formatDirectDnsServers,
  isUsableDirectDns,
  parseDirectDnsServers,
  resolveDirectDnsServers,
  validateDirectDns,
} from "./directDns";

describe("direct DNS helpers", () => {
  it("parses comma and whitespace separated resolver lists", () => {
    expect(parseDirectDnsServers("5.200.200.200, 1.1.1.1")).toEqual([
      "5.200.200.200",
      "1.1.1.1",
    ]);
    expect(formatDirectDnsServers(["5.200.200.200", "1.1.1.1"])).toBe(
      "5.200.200.200, 1.1.1.1",
    );
  });

  it("keeps named presets including private Radar addresses", () => {
    expect(resolveDirectDnsServers("fake_ip", ["1.1.1.1"])).toEqual([]);
    expect(resolveDirectDnsServers("shecan", [])).toEqual([
      ...DIRECT_DNS_PRESET_SERVERS.shecan,
    ]);
    expect(resolveDirectDnsServers("mokhaberat", ["1.1.1.1"])).toEqual([
      "5.200.200.200",
    ]);
    expect(isUsableDirectDns("10.202.10.10")).toBe(true);
    expect(isUsableDirectDns("5.200.200.200")).toBe(true);
  });

  it("rejects loopback, fake-ip, and empty custom lists", () => {
    expect(isUsableDirectDns("127.0.0.1")).toBe(false);
    expect(isUsableDirectDns("198.18.0.1")).toBe(false);
    expect(isUsableDirectDns("dns.google")).toBe(false);
    expect(
      validateDirectDns({
        direct_dns_preset: "fake_ip",
        direct_dns_servers: [],
      }),
    ).toEqual([]);
    expect(
      validateDirectDns({
        direct_dns_preset: "custom",
        direct_dns_servers: [],
      }).some((issue) => issue.code === "DIRECT_DNS_REQUIRED"),
    ).toBe(true);
    expect(
      validateDirectDns({
        direct_dns_preset: "custom",
        direct_dns_servers: ["1.1.1.1"],
      }),
    ).toEqual([]);
  });
});
