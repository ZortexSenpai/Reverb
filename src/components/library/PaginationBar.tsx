// Material-style paginator: a flex sibling *below* the view's scroll area —
// not inside it, so it spans the full width instead of stopping at the
// scrollbar gutter. Right-aligned "rows per page · range · first/prev/next/last".

import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from "lucide-react";
import { useT } from "../../lib/i18n";
import { IconButton } from "../ui/Button";

interface Props {
  page: number; // 0-based
  pageSize: number;
  /** Items on the current page. */
  count: number;
  hasMore: boolean;
  /** Total item count when known (client-side lists); enables "of N" + last-page. */
  total?: number;
  pageSizeOptions?: number[];
  onPageSize?: (size: number) => void;
  onPage: (page: number) => void;
}

export function PaginationBar({
  page,
  pageSize,
  count,
  hasMore,
  total,
  pageSizeOptions,
  onPageSize,
  onPage,
}: Props) {
  const t = useT();
  if (page === 0 && !hasMore) return null;

  const start = page * pageSize + 1;
  const end = page * pageSize + count;
  const lastPage = total != null ? Math.max(0, Math.ceil(total / pageSize) - 1) : null;
  const range = count > 0 ? `${t.n(start)}–${t.n(end)}` : t.n(0);

  return (
    <div className="flex shrink-0 items-center justify-end gap-6 border-t border-edge view-footer px-4 py-1.5">
      {onPageSize && pageSizeOptions && (
        <label className="flex items-center gap-2 text-xs text-dim">
          {t("pagination.perPage")}
          <select
            value={pageSize}
            onChange={(e) => onPageSize(Number(e.target.value))}
            className="cursor-pointer rounded border border-transparent bg-transparent py-1 pr-1 text-xs text-text hover:border-edge focus:border-accent focus:outline-none"
          >
            {pageSizeOptions.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
      )}
      <span className="text-xs tabular-nums text-dim">
        {total != null ? t("pagination.rangeOfTotal", { range, total }) : range}
      </span>
      <div className="flex items-center gap-0.5">
        <IconButton label={t("pagination.firstPage")} onClick={() => onPage(0)} disabled={page === 0}>
          <ChevronsLeft className="size-4" />
        </IconButton>
        <IconButton label={t("pagination.previousPage")} onClick={() => onPage(page - 1)} disabled={page === 0}>
          <ChevronLeft className="size-4" />
        </IconButton>
        <IconButton label={t("pagination.nextPage")} onClick={() => onPage(page + 1)} disabled={!hasMore}>
          <ChevronRight className="size-4" />
        </IconButton>
        {lastPage != null && (
          <IconButton
            label={t("pagination.lastPage")}
            onClick={() => onPage(lastPage)}
            disabled={page >= lastPage}
          >
            <ChevronsRight className="size-4" />
          </IconButton>
        )}
      </div>
    </div>
  );
}
