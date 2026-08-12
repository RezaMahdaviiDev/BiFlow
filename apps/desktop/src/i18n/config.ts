import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const resources = {
  en: {
    translation: {
      appName: "Iran Split",
      dashboard: "Dashboard",
      rules: "Direct rules",
      diagnostics: "Diagnostics",
      settings: "Settings",
      connect: "Connect",
      disconnect: "Disconnect",
      cancel: "Cancel operation",
      direct: "DIRECT",
      vpn: "VPN",
      status: "Status",
      exitIp: "Exit IP",
      backend: "Backend",
      providers: "Providers",
      mockMode: "Mock transport",
      noExitIp: "Available after connection",
      components: "Connection components",
      lastUpdated: "Last updated",
      errors: {
        helperUnavailable: "The privileged helper is unavailable.",
        helperUnauthorized: "The current user is not authorized by the helper.",
        hiddifyNotFound: "Hiddify could not be found.",
        hiddifyEgressUnavailable: "Hiddify is listening but has no usable egress.",
        configInvalid: "The generated runtime configuration is invalid.",
        mihomoStartFailed: "Mihomo could not start.",
        controllerTimeout: "Mihomo's controller did not become ready in time.",
        providerNotReady: "One or more rule providers are not ready.",
        tunCleanupFailed: "The owned TUN or routes could not be completely removed.",
        operationCancelled: "The operation was cancelled.",
        internal: "An internal error occurred.",
      },
    },
  },
  fa: {
    translation: {
      appName: "تقسیم ایران",
      dashboard: "داشبورد",
      rules: "قوانین مستقیم",
      diagnostics: "عیب‌یابی",
      settings: "تنظیمات",
      connect: "اتصال",
      disconnect: "قطع اتصال",
      cancel: "لغو عملیات",
      direct: "مستقیم",
      vpn: "وی‌پی‌ان",
      status: "وضعیت",
      exitIp: "آی‌پی خروجی",
      backend: "بک‌اند",
      providers: "فراهم‌کننده‌های قانون",
      mockMode: "حالت آزمایشی",
      noExitIp: "پس از اتصال نمایش داده می‌شود",
      components: "اجزای اتصال",
      lastUpdated: "آخرین بروزرسانی",
    },
  },
} as const;

void i18n.use(initReactI18next).init({
  resources,
  lng: localStorage.getItem("iran-split-language") ?? "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
