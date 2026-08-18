import { describe, expect, it } from "vitest";
import type { ActiveConnection } from "../api/models";
import { formatGroupIps, groupConnections } from "./connectionGroups";

function conn(overrides: Partial<ActiveConnection>): ActiveConnection {
  return {
    host: "iran.ir",
    destination_ip: "78.38.239.145",
    outbound: "direct",
    rule: "RuleSet(iran-domains)",
    ...overrides,
  };
}

describe("groupConnections", () => {
  it("collapses many sockets to the same host into one row with a count", () => {
    const grouped = groupConnections([conn({}), conn({}), conn({})]);
    expect(grouped).toHaveLength(1);
    expect(grouped[0]?.host).toBe("iran.ir");
    expect(grouped[0]?.count).toBe(3);
  });

  it("keeps hosts with different routes as separate rows", () => {
    const grouped = groupConnections([
      conn({}),
      conn({ outbound: "vpn", rule: "Match" }),
    ]);
    expect(grouped).toHaveLength(2);
  });

  it("collects unique destination IPs per group", () => {
    const grouped = groupConnections([
      conn({}),
      conn({ destination_ip: "78.38.239.146" }),
      conn({ destination_ip: "78.38.239.145" }),
    ]);
    expect(grouped[0]?.ips).toEqual(["78.38.239.145", "78.38.239.146"]);
    expect(grouped[0]?.count).toBe(3);
  });

  it("groups hostless connections by destination IP", () => {
    const grouped = groupConnections([
      conn({ host: "", destination_ip: "1.2.3.4" }),
      conn({ host: "", destination_ip: "1.2.3.4" }),
      conn({ host: "", destination_ip: "5.6.7.8" }),
    ]);
    expect(grouped).toHaveLength(2);
    expect(grouped[0]?.ips).toEqual(["1.2.3.4"]);
    expect(grouped[0]?.count).toBe(2);
  });

  it("sorts busiest groups first, then by name", () => {
    const grouped = groupConnections([
      conn({ host: "b.example" }),
      conn({ host: "a.example" }),
      conn({ host: "chat.example" }),
      conn({ host: "chat.example" }),
    ]);
    expect(grouped.map((group) => group.host)).toEqual([
      "chat.example",
      "a.example",
      "b.example",
    ]);
  });
});

describe("formatGroupIps", () => {
  it("shows a lone IP as-is and summarizes the rest", () => {
    expect(formatGroupIps([])).toBe("");
    expect(formatGroupIps(["1.2.3.4"])).toBe("1.2.3.4");
    expect(formatGroupIps(["1.2.3.4", "5.6.7.8", "9.9.9.9"])).toBe(
      "1.2.3.4 +2",
    );
  });
});
