import { describe, expect, it } from "vitest";
import { isEditableTarget, selectionLength } from "./editableTarget";

describe("editableTarget", () => {
  it("accepts text and number inputs and textareas", () => {
    const text = document.createElement("input");
    text.type = "text";
    const number = document.createElement("input");
    number.type = "number";
    const area = document.createElement("textarea");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    expect(isEditableTarget(text)).toBe(true);
    expect(isEditableTarget(number)).toBe(true);
    expect(isEditableTarget(area)).toBe(true);
    expect(isEditableTarget(checkbox)).toBe(false);
    expect(isEditableTarget(document.body)).toBe(false);
  });

  it("measures a text selection", () => {
    const field = document.createElement("input");
    field.value = "abcdef";
    field.setSelectionRange(1, 4);
    expect(selectionLength(field)).toBe(3);
  });
});
