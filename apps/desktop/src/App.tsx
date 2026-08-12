import * as Dialog from "@radix-ui/react-dialog";
import { Activity, BookOpen, Languages, LayoutDashboard, Moon, SettingsIcon, Sun, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dashboard } from "./components/Dashboard";
import { Diagnostics } from "./components/Diagnostics";
import { DirectRules } from "./components/DirectRules";
import { Settings } from "./components/Settings";
import { useAppStore } from "./store/app";

export function App() {
  const { i18n, t } = useTranslation();
  const store = useAppStore();
  const [dark, setDark] = useState(() => localStorage.getItem("iran-split-theme") === "dark");

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    localStorage.setItem("iran-split-theme", dark ? "dark" : "light");
  }, [dark]);

  useEffect(() => {
    const rtl = i18n.language === "fa";
    document.documentElement.dir = rtl ? "rtl" : "ltr";
    document.documentElement.lang = i18n.language;
  }, [i18n.language]);

  useEffect(() => {
    let unsubscribe = () => undefined;
    void store.initialize().then((result) => {
      unsubscribe = result;
    });
    return () => unsubscribe();
    // The Zustand action is stable for the store lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (store.loading) {
    return (
      <main className="grid min-h-screen place-items-center" aria-busy="true">
        <Activity className="animate-pulse text-brand" size={32} aria-label="Loading" />
      </main>
    );
  }

  return (
    <div className="min-h-screen lg:grid lg:grid-cols-[15rem_1fr]">
      <aside className="border-b border-ink/10 bg-surface/85 p-4 backdrop-blur lg:min-h-screen lg:border-b-0 lg:border-r">
        <div className="flex items-center justify-between lg:block">
          <div className="flex items-center gap-3 px-2 py-2">
            <div className="grid h-10 w-10 place-items-center rounded-xl bg-brand text-lg font-black text-white">IR</div>
            <div>
              <p className="font-semibold">{t("appName")}</p>
              <p className="text-xs text-muted">Split routing control</p>
            </div>
          </div>
          <div className="flex gap-1 lg:hidden">
            <ThemeButton dark={dark} setDark={setDark} />
            <LanguageButton language={i18n.language} change={(lng) => void i18n.changeLanguage(lng)} />
          </div>
        </div>
        <nav aria-label="Primary navigation" className="mt-4 grid grid-cols-4 gap-1 lg:grid-cols-1">
          <NavButton page="dashboard" label={t("dashboard")} icon={<LayoutDashboard />} />
          <NavButton page="rules" label={t("rules")} icon={<BookOpen />} />
          <NavButton page="diagnostics" label={t("diagnostics")} icon={<Activity />} />
          <NavButton page="settings" label={t("settings")} icon={<SettingsIcon />} />
        </nav>
        <div className="mt-auto hidden gap-1 px-1 pt-8 lg:flex">
          <ThemeButton dark={dark} setDark={setDark} />
          <LanguageButton language={i18n.language} change={(lng) => void i18n.changeLanguage(lng)} />
        </div>
      </aside>

      <main className="mx-auto w-full max-w-6xl p-5 sm:p-8 lg:p-10">
        {store.page === "dashboard" && store.snapshot ? <Dashboard snapshot={store.snapshot} /> : null}
        {store.page === "rules" && store.rules ? <DirectRules rules={store.rules} /> : null}
        {store.page === "diagnostics" ? <Diagnostics report={store.diagnostics} /> : null}
        {store.page === "settings" && store.settings ? <Settings settings={store.settings} /> : null}
      </main>

      <Dialog.Root open={store.error !== null} onOpenChange={(open) => !open && store.clearError()}>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 bg-black/45" />
          <Dialog.Content className="fixed left-1/2 top-1/2 w-[min(32rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 rounded-2xl bg-surface p-6 shadow-2xl">
            <Dialog.Title className="text-lg font-semibold">Action could not be completed</Dialog.Title>
            <Dialog.Description className="mt-2 text-sm text-muted">{store.error}</Dialog.Description>
            {store.snapshot?.last_error ? (
              <p className="mt-3 rounded-lg bg-canvas p-3 font-mono text-xs text-muted">
                Correlation ID: {store.snapshot.last_error.correlation_id}
              </p>
            ) : null}
            <Dialog.Close className="absolute right-4 top-4 rounded-lg p-1 text-muted" aria-label="Close">
              <X size={19} aria-hidden />
            </Dialog.Close>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  );
}

function NavButton({ page, label, icon }: { page: "dashboard" | "rules" | "diagnostics" | "settings"; label: string; icon: React.ReactNode }) {
  const { page: current, setPage } = useAppStore();
  const active = current === page;
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      onClick={() => setPage(page)}
      className={`flex min-w-0 items-center justify-center gap-2 rounded-xl px-3 py-3 text-sm font-medium transition lg:justify-start ${
        active ? "bg-brand/10 text-brand" : "text-muted hover:bg-ink/5 hover:text-ink"
      }`}
    >
      <span aria-hidden>{icon}</span>
      <span className="hidden truncate sm:inline">{label}</span>
    </button>
  );
}

function ThemeButton({ dark, setDark }: { dark: boolean; setDark: (dark: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => setDark(!dark)}
      className="rounded-xl p-2 text-muted hover:bg-ink/5 hover:text-ink"
      aria-label={dark ? "Use light theme" : "Use dark theme"}
    >
      {dark ? <Sun size={19} aria-hidden /> : <Moon size={19} aria-hidden />}
    </button>
  );
}

function LanguageButton({ language, change }: { language: string; change: (language: string) => void }) {
  return (
    <button
      type="button"
      onClick={() => {
        const next = language === "fa" ? "en" : "fa";
        localStorage.setItem("iran-split-language", next);
        change(next);
      }}
      className="rounded-xl p-2 text-muted hover:bg-ink/5 hover:text-ink"
      aria-label="Change language"
    >
      <Languages size={19} aria-hidden />
    </button>
  );
}
