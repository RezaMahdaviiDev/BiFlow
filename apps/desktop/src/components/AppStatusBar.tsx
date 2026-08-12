import { LoaderCircle, MapPin, Wifi, WifiOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { NetworkStatus } from "../api/models";
import { useAppStore } from "../store/app";
import { countryFlag } from "./country";

function countryName(code: string | null, language: string): string | null {
  if (!code) return null;
  try {
    return (
      new Intl.DisplayNames([language], { type: "region" }).of(code) ?? code
    );
  } catch {
    return code;
  }
}

function locationLabel(status: NetworkStatus, language: string): string | null {
  const country = countryName(status.country_code, language);
  return [status.city, country].filter(Boolean).join(", ") || null;
}

export function AppStatusBar() {
  const { i18n, t } = useTranslation();
  const status = useAppStore((state) => state.networkStatus);
  const current: NetworkStatus = status ?? {
    state: "checking",
    public_ip: null,
    country_code: null,
    city: null,
    checked_at: new Date().toISOString(),
    detail: null,
  };
  const location = locationLabel(current, i18n.language);
  const flag = countryFlag(current.country_code);

  return (
    <footer
      className="border-t border-ink/10 bg-surface/90 px-5 py-3 text-xs text-muted backdrop-blur sm:px-8 lg:px-10"
      role="status"
      aria-live="polite"
      title={current.detail ?? undefined}
    >
      <div className="mx-auto flex w-full max-w-6xl flex-wrap items-center gap-x-5 gap-y-2">
        <span className="inline-flex items-center gap-2 font-semibold text-ink">
          {current.state === "online" ? (
            <Wifi className="text-success" size={16} aria-hidden />
          ) : current.state === "offline" ? (
            <WifiOff className="text-danger" size={16} aria-hidden />
          ) : (
            <LoaderCircle
              className="animate-spin text-muted"
              size={16}
              aria-hidden
            />
          )}
          {t(`internet.${current.state}`)}
        </span>
        {current.public_ip ? (
          <span>
            {t("currentIp")}:{" "}
            <bdi className="font-mono text-ink">{current.public_ip}</bdi>
          </span>
        ) : null}
        {location ? (
          <span className="inline-flex items-center gap-1.5">
            <MapPin size={14} aria-hidden />
            {flag ? <span aria-hidden>{flag}</span> : null}
            <span>{location}</span>
          </span>
        ) : null}
      </div>
    </footer>
  );
}
