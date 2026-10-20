import { Search as SearchIcon, X } from "lucide-react";
import { useT } from "../../lib/i18n";
import { Input } from "../ui/Input";

interface FilterInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  /**
   * Layout utilities for the wrapper (width, margins); the input fills it.
   * Header rows pass `ml-auto` to sit the field flush right.
   */
  className?: string;
}

/** Search-icon filter box shared by the library list views. */
export function FilterInput({ value, onChange, placeholder, className = "w-56" }: FilterInputProps) {
  const t = useT();
  return (
    <div className={`relative ${className}`}>
      <SearchIcon className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-faint" />
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape" && value !== "") {
            e.stopPropagation();
            onChange("");
          }
        }}
        placeholder={placeholder}
        spellCheck={false}
        className={value === "" ? "pl-9" : "pr-9 pl-9"}
      />
      {value !== "" && (
        <button
          type="button"
          aria-label={t("common.clearFilter")}
          onClick={() => onChange("")}
          className="absolute top-1/2 right-1.5 cursor-pointer rounded-full p-1 text-dim transition-colors hover:bg-surface-2 hover:text-text -translate-y-1/2"
        >
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}
