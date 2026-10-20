// A multi-select in the Material mould: a select-shaped trigger that lists the
// chosen labels, opening a panel of checkbox rows.
//
// The point of it over a native `<select multiple>` is the toggling. There, a
// click without a modifier replaces the whole selection, so one stray click
// wipes everything the user picked; here each row toggles on its own and the
// panel stays open, so a selection can be built up and corrected in place.

import { Popover } from "radix-ui";
import { Check, ChevronDown } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useT } from "../../lib/i18n";

export interface MultiSelectOption {
  value: string;
  label: string;
  icon?: LucideIcon;
}

/**
 * `selected` with `value` added or removed. The result is rebuilt from
 * `options`, so it keeps a stable, declared order however the user clicked
 * their way to it — callers persist this, and an order that shuffled with every
 * click would churn the stored value for no reason.
 */
export function nextSelection(
  options: MultiSelectOption[],
  selected: string[],
  value: string,
): string[] {
  const next = new Set(selected);
  if (!next.delete(value)) {
    next.add(value);
  }
  return options.filter((option) => next.has(option.value)).map((option) => option.value);
}

export function MultiSelect({
  options,
  selected,
  onChange,
  label,
  emptyLabel,
  className = "w-56",
}: {
  options: MultiSelectOption[];
  /** Selected values; order is taken from `options`, not from this array. */
  selected: string[];
  onChange: (selected: string[]) => void;
  /** Accessible name for the trigger. */
  label: string;
  /** Trigger text when nothing is selected; defaults to "None". */
  emptyLabel?: string;
  className?: string;
}) {
  const t = useT();
  const chosen = options.filter((option) => selected.includes(option.value));

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          aria-label={label}
          className={`flex cursor-pointer items-center justify-between gap-2 rounded-md border border-edge bg-bg px-2.5 py-1.5 text-sm text-text transition-colors hover:border-dim focus:border-accent focus:outline-none ${className}`}
        >
          <span className={`truncate ${chosen.length === 0 ? "text-faint" : ""}`}>
            {chosen.length === 0
              ? (emptyLabel ?? t("common.none"))
              : chosen.map((option) => option.label).join(", ")}
          </span>
          <ChevronDown className="size-4 shrink-0 text-faint" />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          role="listbox"
          aria-multiselectable
          aria-label={label}
          className="z-50 max-h-72 w-56 overflow-y-auto rounded-lg border border-edge bg-surface p-1 shadow-xl focus:outline-none"
        >
          {options.map(({ value, label: optionLabel, icon: Icon }) => {
            const isSelected = selected.includes(value);
            return (
              <button
                key={value}
                role="option"
                aria-selected={isSelected}
                onClick={() => onChange(nextSelection(options, selected, value))}
                className="flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-sm text-text transition-colors hover:bg-surface-2"
              >
                <span
                  aria-hidden
                  className={`flex size-4 shrink-0 items-center justify-center rounded-sm border transition-colors ${
                    isSelected ? "border-accent bg-accent text-white" : "border-edge"
                  }`}
                >
                  {isSelected && <Check className="size-3" strokeWidth={3} />}
                </span>
                {Icon && <Icon className="size-4 shrink-0 text-faint" />}
                <span className="truncate">{optionLabel}</span>
              </button>
            );
          })}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
