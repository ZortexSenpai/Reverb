// Small controlled dialogs: a text prompt and a confirm, both Radix-based.

import { Dialog } from "radix-ui";
import { useEffect, useState } from "react";
import { useT } from "../../lib/i18n";
import { Button } from "./Button";
import { Input } from "./Input";

interface TextPromptProps {
  open: boolean;
  title: string;
  placeholder?: string;
  initialValue?: string;
  /** Defaults to "Save" in the current language. */
  confirmLabel?: string;
  onConfirm: (value: string) => void;
  onClose: () => void;
}

export function TextPromptDialog({
  open,
  title,
  placeholder,
  initialValue = "",
  confirmLabel,
  onConfirm,
  onClose,
}: TextPromptProps) {
  const t = useT();
  const [value, setValue] = useState(initialValue);
  useEffect(() => {
    if (open) setValue(initialValue);
  }, [open, initialValue]);

  function submit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = value.trim();
    if (trimmed) {
      onConfirm(trimmed);
      onClose();
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60" />
        <Dialog.Content
          className="fixed top-1/2 left-1/2 z-50 w-96 -translate-x-1/2 -translate-y-1/2 rounded-xl border border-edge bg-surface p-5 shadow-2xl focus:outline-none"
          aria-describedby={undefined}
        >
          <Dialog.Title className="mb-3 text-sm font-semibold">{title}</Dialog.Title>
          <form onSubmit={submit}>
            <Input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder={placeholder}
              autoFocus
              spellCheck={false}
            />
            <div className="mt-4 flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={onClose}>
                {t("common.cancel")}
              </Button>
              <Button type="submit" disabled={!value.trim()}>
                {confirmLabel ?? t("common.save")}
              </Button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

interface ConfirmProps {
  open: boolean;
  title: string;
  body?: string;
  /** Defaults to "Confirm" in the current language. */
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel,
  danger = false,
  onConfirm,
  onClose,
}: ConfirmProps) {
  const t = useT();
  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60" />
        <Dialog.Content
          className="fixed top-1/2 left-1/2 z-50 w-96 -translate-x-1/2 -translate-y-1/2 rounded-xl border border-edge bg-surface p-5 shadow-2xl focus:outline-none"
          aria-describedby={undefined}
        >
          <Dialog.Title className="mb-2 text-sm font-semibold">{title}</Dialog.Title>
          {body && <p className="mb-4 text-sm text-dim">{body}</p>}
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              onClick={() => {
                onConfirm();
                onClose();
              }}
              className={danger ? "bg-danger hover:bg-danger/80" : ""}
            >
              {confirmLabel ?? t("common.confirm")}
            </Button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
