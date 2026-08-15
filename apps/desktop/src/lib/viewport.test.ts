import { describe, expect, it, vi } from "vitest";
import { isMobileViewport, subscribeMobileViewport } from "./viewport";

describe("viewport", () => {
  it("treats a missing matchMedia as desktop", () => {
    const original = window.matchMedia;
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: undefined,
    });
    expect(isMobileViewport()).toBe(false);
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: original,
    });
  });

  it("reports the current max-width: 767px media query", () => {
    window.matchMedia = vi.fn().mockReturnValue({
      matches: true,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }) as unknown as typeof window.matchMedia;
    expect(isMobileViewport()).toBe(true);
  });

  it("unsubscribes the media listener", () => {
    const removeEventListener = vi.fn();
    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener,
    }) as unknown as typeof window.matchMedia;
    const stop = subscribeMobileViewport(vi.fn());
    stop();
    expect(removeEventListener).toHaveBeenCalledOnce();
  });
});
