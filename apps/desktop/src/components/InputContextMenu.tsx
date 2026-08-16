import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { isEditableTarget, selectionLength } from "../lib/editableTarget";

interface MenuState {
  x: number;
  y: number;
  field: HTMLInputElement | HTMLTextAreaElement;
}

export function InputContextMenu() {
  const { t } = useTranslation();
  const [menu, setMenu] = useState<MenuState | null>(null);

  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      if (!isEditableTarget(event.target)) {
        setMenu(null);
        return;
      }
      event.preventDefault();
      event.target.focus();
      setMenu({
        x: event.clientX,
        y: event.clientY,
        field: event.target,
      });
    };
    const dismiss = () => setMenu(null);
    document.addEventListener("contextmenu", onContextMenu);
    document.addEventListener("click", dismiss);
    window.addEventListener("blur", dismiss);
    window.addEventListener("resize", dismiss);
    return () => {
      document.removeEventListener("contextmenu", onContextMenu);
      document.removeEventListener("click", dismiss);
      window.removeEventListener("blur", dismiss);
      window.removeEventListener("resize", dismiss);
    };
  }, []);

  if (!menu) {
    return null;
  }

  const readonly = menu.field.readOnly || menu.field.disabled;
  const hasSelection = selectionLength(menu.field) > 0;

  return (
    <ul
      role="menu"
      data-testid="input-context-menu"
      className="fixed z-[80] min-w-40 rounded-lg border border-ink/10 bg-surface py-1 text-sm shadow-xl"
      style={{ left: menu.x, top: menu.y }}
      onClick={(event) => event.stopPropagation()}
    >
      <MenuItem
        label={t("contextSelectAll")}
        disabled={menu.field.disabled}
        onSelect={() => {
          menu.field.focus();
          menu.field.select();
        }}
      />
      <MenuItem
        label={t("contextCopy")}
        disabled={!hasSelection}
        onSelect={() => {
          void copySelection(menu.field);
        }}
      />
      <MenuItem
        label={t("contextCut")}
        disabled={readonly || !hasSelection}
        onSelect={() => {
          void cutSelection(menu.field);
        }}
      />
      <MenuItem
        label={t("contextPaste")}
        disabled={readonly}
        onSelect={() => {
          void pasteInto(menu.field);
        }}
      />
    </ul>
  );
}

function MenuItem({
  label,
  disabled,
  onSelect,
}: {
  label: string;
  disabled: boolean;
  onSelect: () => void;
}) {
  return (
    <li role="none">
      <button
        type="button"
        role="menuitem"
        disabled={disabled}
        className="block w-full px-3 py-1.5 text-start disabled:text-muted"
        onClick={onSelect}
      >
        {label}
      </button>
    </li>
  );
}

async function copySelection(
  field: HTMLInputElement | HTMLTextAreaElement,
): Promise<void> {
  const text = field.value.slice(
    field.selectionStart ?? 0,
    field.selectionEnd ?? 0,
  );
  if (text) {
    await navigator.clipboard.writeText(text);
  }
}

async function cutSelection(
  field: HTMLInputElement | HTMLTextAreaElement,
): Promise<void> {
  await copySelection(field);
  const start = field.selectionStart ?? 0;
  const end = field.selectionEnd ?? 0;
  replaceRange(field, start, end, "");
}

async function pasteInto(
  field: HTMLInputElement | HTMLTextAreaElement,
): Promise<void> {
  const text = await navigator.clipboard.readText();
  const start = field.selectionStart ?? field.value.length;
  const end = field.selectionEnd ?? field.value.length;
  replaceRange(field, start, end, text);
}

function replaceRange(
  field: HTMLInputElement | HTMLTextAreaElement,
  start: number,
  end: number,
  insert: string,
): void {
  const next = field.value.slice(0, start) + insert + field.value.slice(end);
  field.value = next;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  const caret = start + insert.length;
  field.setSelectionRange(caret, caret);
}
