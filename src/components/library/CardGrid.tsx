// Row-virtualized card grid shared by the albums and artists views: renders
// only visible rows, so 10k+ item libraries stay smooth. Column count derives
// from container width via ResizeObserver.

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";

const GAP = 4;

interface Props<T> {
  items: T[];
  itemKey: (item: T) => string;
  renderItem: (item: T) => ReactNode;
  scrollRef: RefObject<HTMLDivElement | null>;
  onEndReached?: () => void;
  /** Minimum card width; columns are derived from it. */
  cardMinWidth?: number;
  /** Vertical space under the (square) image for text. */
  rowExtra?: number;
}

export function CardGrid<T>({
  items,
  itemKey,
  renderItem,
  scrollRef,
  onEndReached,
  cardMinWidth = 190,
  rowExtra = 72,
}: Props<T>) {
  const [width, setWidth] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => setWidth(entries[0].contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const columns = Math.max(2, Math.floor((width + GAP) / (cardMinWidth + GAP)));
  const rowHeight = (width - GAP * (columns - 1)) / columns + rowExtra;
  const rowCount = Math.ceil(items.length / columns);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 3,
  });

  const virtualItems = virtualizer.getVirtualItems();

  // Infinite scroll: fire when the last rendered row nears the end.
  const lastItemIndex =
    virtualItems.length > 0 ? virtualItems[virtualItems.length - 1].index : -1;
  useEffect(() => {
    if (onEndReached && lastItemIndex >= 0 && lastItemIndex >= rowCount - 2) {
      onEndReached();
    }
  }, [lastItemIndex, rowCount, onEndReached]);

  const rows = useMemo(() => {
    return virtualItems.map((vRow) => {
      const start = vRow.index * columns;
      return {
        key: vRow.key,
        start: vRow.start,
        items: items.slice(start, start + columns),
      };
    });
  }, [virtualItems, items, columns]);

  return (
    <div ref={containerRef} className="relative" style={{ height: virtualizer.getTotalSize() }}>
      {width > 0 &&
        rows.map((row) => (
          <div
            key={row.key}
            className="absolute right-0 left-0 grid"
            style={{
              top: row.start,
              height: rowHeight,
              gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
              gap: GAP,
            }}
          >
            {row.items.map((item) => (
              <div key={itemKey(item)}>{renderItem(item)}</div>
            ))}
          </div>
        ))}
    </div>
  );
}
