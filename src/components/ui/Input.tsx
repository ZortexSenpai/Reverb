import type { ComponentProps } from "react";

export function Input({ className = "", ...rest }: ComponentProps<"input">) {
  return (
    <input
      className={`w-full rounded-md border border-edge bg-surface px-3 py-2 text-sm text-text placeholder:text-faint focus:border-accent focus:outline-none ${className}`}
      {...rest}
    />
  );
}
