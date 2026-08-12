export function countryFlag(code: string | null): string {
  if (!code || !/^[a-z]{2}$/i.test(code)) return "";
  return [...code.toUpperCase()]
    .map((letter) => String.fromCodePoint(127_397 + letter.charCodeAt(0)))
    .join("");
}
