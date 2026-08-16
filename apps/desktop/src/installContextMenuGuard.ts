import { isEditableTarget } from "./lib/editableTarget";

export function installContextMenuGuard(): void {
  document.addEventListener(
    "contextmenu",
    (event) => {
      if (isEditableTarget(event.target)) {
        return;
      }
      event.preventDefault();
    },
    { capture: true },
  );
}
