import type { DirectDnsPreset, ValidationIssue } from "../api/models";

export const DIRECT_DNS_PRESETS = [
  "fake_ip",
  "shecan",
  "electro",
  "radar",
  "mokhaberat",
  "custom",
] as const;

export const DIRECT_DNS_PRESET_SERVERS: Record<
  Exclude<DirectDnsPreset, "fake_ip" | "custom">,
  readonly string[]
> = {
  shecan: ["178.22.122.100", "185.51.200.2"],
  electro: ["78.157.42.100", "78.157.42.101"],
  radar: ["10.202.10.10", "10.202.10.11"],
  mokhaberat: ["5.200.200.200"],
};

export function parseDirectDnsServers(input: string): string[] {
  return input
    .split(/[\s,]+/)
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

export function formatDirectDnsServers(servers: string[]): string {
  return servers.join(", ");
}

export function resolveDirectDnsServers(
  preset: DirectDnsPreset,
  customServers: string[],
): string[] {
  if (preset === "fake_ip") return [];
  if (preset === "custom") {
    return customServers.map((server) => server.trim()).filter(Boolean);
  }
  return [...DIRECT_DNS_PRESET_SERVERS[preset]];
}

export function isUsableDirectDns(value: string): boolean {
  const ipv4 = parseIpv4(value);
  if (ipv4) {
    const [a, b] = ipv4;
    if (a === undefined || b === undefined) return false;
    if (a === 127 || a === 0) return false;
    if (a >= 224 && a <= 239) return false;
    if (a === 198 && (b === 18 || b === 19)) return false;
    return true;
  }
  if (!value.includes(":")) return false;
  const lower = value.toLowerCase();
  if (lower === "::" || lower === "::1") return false;
  if (lower.startsWith("ff")) return false;
  return /^[0-9a-f:]+$/i.test(value);
}

export function validateDirectDns(mihomo: {
  direct_dns_preset: DirectDnsPreset;
  direct_dns_servers: string[];
}): ValidationIssue[] {
  if (mihomo.direct_dns_preset === "fake_ip") return [];
  const servers = resolveDirectDnsServers(
    mihomo.direct_dns_preset,
    mihomo.direct_dns_servers,
  );
  if (servers.length === 0) {
    return [
      {
        field: "mihomo.direct_dns_servers",
        code: "DIRECT_DNS_REQUIRED",
        message: "DIRECT DNS needs at least one resolver address",
      },
    ];
  }
  const issues: ValidationIssue[] = [];
  if (servers.length > 4) {
    issues.push({
      field: "mihomo.direct_dns_servers",
      code: "DIRECT_DNS_TOO_MANY",
      message: "DIRECT DNS accepts at most four resolver addresses",
    });
  }
  for (const server of servers) {
    if (!isUsableDirectDns(server)) {
      issues.push({
        field: "mihomo.direct_dns_servers",
        code: "DIRECT_DNS_INVALID",
        message:
          "DIRECT DNS resolvers must be IP addresses, not loopback, multicast, or fake-ip",
      });
    }
  }
  return issues;
}

function parseIpv4(value: string): number[] | null {
  const parts = value.split(".");
  if (parts.length !== 4) return null;
  const octets = parts.map((part) => {
    if (!/^\d{1,3}$/.test(part)) return Number.NaN;
    return Number(part);
  });
  if (
    octets.some((octet) => !Number.isInteger(octet) || octet < 0 || octet > 255)
  ) {
    return null;
  }
  return octets;
}
