import { DropdownMenu } from "radix-ui";
import { ChevronDown, Save, Trash2 } from "lucide-react";
import { useState } from "react";
import { useT } from "../../lib/i18n";
import { useEqStore } from "../../stores/eqStore";

export function EqPresetMenu() {
  const t = useT();
  const presets = useEqStore((s) => s.presets);
  const activePresetId = useEqStore((s) => s.activePresetId);
  const dirty = useEqStore((s) => s.dirty);
  const { loadPreset, saveAsPreset, deletePreset } = useEqStore.getState();
  const [newName, setNewName] = useState("");

  const active = presets.find((p) => p.id === activePresetId);

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button className="flex cursor-pointer items-center gap-1.5 rounded-md border border-edge bg-bg px-3 py-1.5 text-xs transition-colors hover:bg-surface-2">
          {active ? `${active.name}${dirty ? " *" : ""}` : t("eq.presets")}
          <ChevronDown className="size-3.5 text-dim" />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="end"
          className="z-[70] min-w-56 rounded-lg border border-edge bg-surface p-1 shadow-xl"
        >
          {presets.length === 0 && (
            <div className="px-2.5 py-2 text-xs text-faint">{t("eq.noPresets")}</div>
          )}
          {presets.map((preset) => (
            <div key={preset.id} className="group flex items-center">
              <DropdownMenu.Item
                className="flex-1 cursor-pointer rounded px-2.5 py-1.5 text-sm outline-none select-none data-highlighted:bg-surface-2"
                onSelect={() => void loadPreset(preset.id)}
              >
                {preset.name}
                {preset.id === activePresetId && (
                  <span className="ml-1 text-eq-curve">{dirty ? "*" : "•"}</span>
                )}
              </DropdownMenu.Item>
              <button
                aria-label={t("eq.deletePreset", { name: preset.name })}
                onClick={(e) => {
                  e.stopPropagation();
                  void deletePreset(preset.id);
                }}
                className="cursor-pointer rounded p-1.5 text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-danger"
              >
                <Trash2 className="size-3.5" />
              </button>
            </div>
          ))}
          <DropdownMenu.Separator className="my-1 h-px bg-edge" />
          <form
            className="flex items-center gap-1 p-1"
            onSubmit={(e) => {
              e.preventDefault();
              const name = newName.trim();
              if (name) {
                void saveAsPreset(name);
                setNewName("");
              }
            }}
          >
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={t("eq.savePlaceholder")}
              onKeyDown={(e) => e.stopPropagation()}
              className="w-full rounded border border-edge bg-bg px-2 py-1 text-xs focus:border-accent focus:outline-none"
            />
            <button
              type="submit"
              aria-label={t("eq.savePreset")}
              disabled={!newName.trim()}
              className="cursor-pointer rounded p-1.5 text-dim transition-colors hover:text-text disabled:opacity-40"
            >
              <Save className="size-4" />
            </button>
          </form>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
