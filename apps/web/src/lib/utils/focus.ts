/** Keep Tab cycling inside a container (dialogs, menus). Returns true when handled. */
export function wrapTabFocus(container: HTMLElement, e: KeyboardEvent): boolean {
  if (e.key !== 'Tab') return false;
  const items = [...container.querySelectorAll<HTMLElement>(
    'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
  )].filter((el) => !el.hasAttribute('disabled'));
  if (items.length === 0) {
    e.preventDefault();
    return true;
  }
  const first = items[0];
  const last = items[items.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
    return true;
  }
  if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
    return true;
  }
  return false;
}
