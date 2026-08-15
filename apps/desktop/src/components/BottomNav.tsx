import {
  Activity,
  BookOpen,
  Info,
  LayoutDashboard,
  SettingsIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../store/app";

const items = [
  { page: "dashboard", icon: LayoutDashboard, labelKey: "dashboard" },
  { page: "rules", icon: BookOpen, labelKey: "rules" },
  { page: "diagnostics", icon: Activity, labelKey: "diagnostics" },
  { page: "settings", icon: SettingsIcon, labelKey: "settings" },
  { page: "about", icon: Info, labelKey: "about" },
] as const;

export function BottomNav() {
  const { t } = useTranslation();
  const { page: current, setPage } = useAppStore();

  return (
    <nav
      data-testid="bottom-nav"
      aria-label="Primary navigation"
      className="app-bottom-nav flex shrink-0 border-t border-ink/10 bg-surface/95 px-1 py-1 md:hidden"
    >
      {items.map(({ page, icon: Icon, labelKey }) => {
        const active = current === page;
        return (
          <button
            key={page}
            type="button"
            aria-current={active ? "page" : undefined}
            onClick={() => setPage(page)}
            className={`flex min-w-0 flex-1 flex-col items-center gap-0.5 rounded-lg px-1 py-2 text-[0.65rem] font-medium ${
              active ? "text-brand" : "text-muted"
            }`}
          >
            <Icon size={18} aria-hidden />
            <span className="max-w-full truncate">{t(labelKey)}</span>
          </button>
        );
      })}
    </nav>
  );
}
