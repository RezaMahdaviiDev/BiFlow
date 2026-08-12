/// <reference types="vite/client" />

declare module "*.png" {
  const src: string;
  export default src;
}

declare global {
  const __APP_VERSION__: string;
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __BIFLOW_RESET_MOCK?: () => void;
  }
}

export {};
