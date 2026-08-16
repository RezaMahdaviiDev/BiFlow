const MOBILE_QUERY = "(max-width: 767px)";

export function isMobileViewport(): boolean {
  if (
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function"
  ) {
    return false;
  }
  return window.matchMedia(MOBILE_QUERY).matches;
}

export function subscribeMobileViewport(
  listener: (mobile: boolean) => void,
): () => void {
  if (
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function"
  ) {
    return () => undefined;
  }
  const media = window.matchMedia(MOBILE_QUERY);
  const onChange = () => listener(media.matches);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}
