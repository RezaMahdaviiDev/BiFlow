const TEXT_INPUT_TYPES = new Set([
  "text",
  "search",
  "url",
  "tel",
  "password",
  "number",
  "email",
]);

export function isEditableTarget(
  target: EventTarget | null,
): target is HTMLInputElement | HTMLTextAreaElement {
  if (target instanceof HTMLTextAreaElement) {
    return true;
  }
  if (target instanceof HTMLInputElement) {
    return TEXT_INPUT_TYPES.has(target.type.toLowerCase());
  }
  return false;
}

export function selectionLength(
  field: HTMLInputElement | HTMLTextAreaElement,
): number {
  if (
    typeof field.selectionStart === "number" &&
    typeof field.selectionEnd === "number"
  ) {
    return Math.max(0, field.selectionEnd - field.selectionStart);
  }
  return 0;
}
