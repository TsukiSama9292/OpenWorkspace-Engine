/** A single destructive-or-significant confirmation request. Rendered by ConfirmDialog. */
export interface PendingConfirm {
  title: string;
  body: string;
  confirmLabel: string;
  danger: boolean;
  onConfirm: () => void;
}
