import { useT } from "../../lib/i18n";

export function Spinner({ className = "" }: { className?: string }) {
  const t = useT();
  return (
    <div
      className={`size-6 animate-spin rounded-full border-2 border-edge border-t-accent ${className}`}
      role="status"
      aria-label={t("common.loading")}
    />
  );
}

export function CenteredSpinner() {
  return (
    <div className="flex h-full items-center justify-center py-16">
      <Spinner />
    </div>
  );
}
