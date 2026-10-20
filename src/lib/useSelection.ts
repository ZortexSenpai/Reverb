// Row multi-selection for track lists: plain click replaces, ctrl/meta-click
// toggles, shift-click extends from the anchor — the conventions every file
// manager and music player shares.

import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";

export interface Selection<T> {
  count: number;
  isSelected: (id: string) => boolean;
  /** The selected items, in list order. */
  selected: T[];
  onRowClick: (index: number, e: MouseEvent) => void;
  /** Right-click keeps a multi-selection the row belongs to, else replaces it. */
  onRowContextMenu: (index: number) => void;
  /** What an action on `item` should apply to: the selection, or just it. */
  targetsFor: (item: T) => T[];
  clear: () => void;
}

export interface ClickModifiers {
  /** Extend from the anchor. */
  shift: boolean;
  /** Ctrl/meta: add or remove this one row. */
  toggle: boolean;
}

/**
 * Where a click leaves the selection. Pure so the interaction rules can be
 * tested directly; the hook only owns the state around it.
 */
export function resolveClick<T extends { id: string }>(
  items: T[],
  index: number,
  mods: ClickModifiers,
  selected: ReadonlySet<string>,
  anchor: number | null,
): { ids: Set<string>; anchor: number | null } {
  const item = items[index];
  if (!item) return { ids: new Set(selected), anchor };

  if (mods.shift && anchor !== null && anchor < items.length) {
    const [from, to] = anchor <= index ? [anchor, index] : [index, anchor];
    return { ids: new Set(items.slice(from, to + 1).map((it) => it.id)), anchor };
  }
  if (mods.toggle) {
    const ids = new Set(selected);
    if (!ids.delete(item.id)) ids.add(item.id);
    return { ids, anchor: index };
  }
  return { ids: new Set([item.id]), anchor: index };
}

/** Drop ids that are no longer in the list; same Set back when nothing changed. */
export function pruneSelection<T extends { id: string }>(
  items: T[],
  selected: ReadonlySet<string>,
): ReadonlySet<string> {
  if (selected.size === 0) return selected;
  const live = new Set(items.map((item) => item.id));
  const next = new Set([...selected].filter((id) => live.has(id)));
  return next.size === selected.size ? selected : next;
}

export function useSelection<T extends { id: string }>(items: T[]): Selection<T> {
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(() => new Set());
  const anchor = useRef<number | null>(null);

  // Paging, filtering and sorting all hand us a new list; anything no longer
  // in it can't be acted on, so drop it rather than act on a stale id.
  useEffect(() => {
    setSelectedIds((prev) => pruneSelection(items, prev));
  }, [items]);

  const clear = useCallback(() => {
    anchor.current = null;
    setSelectedIds((prev) => (prev.size === 0 ? prev : new Set()));
  }, []);

  const onRowClick = useCallback(
    (index: number, e: MouseEvent) => {
      const mods = { shift: e.shiftKey, toggle: e.ctrlKey || e.metaKey };
      setSelectedIds((prev) => {
        const next = resolveClick(items, index, mods, prev, anchor.current);
        anchor.current = next.anchor;
        return next.ids;
      });
    },
    [items],
  );

  const onRowContextMenu = useCallback(
    (index: number) => {
      const item = items[index];
      if (!item) return;
      setSelectedIds((prev) => {
        if (prev.has(item.id)) return prev;
        anchor.current = index;
        return new Set([item.id]);
      });
    },
    [items],
  );

  // Only bind once something is selected: before that the list has no claim
  // on Escape or ctrl+A, which belong to whatever else is focused.
  useEffect(() => {
    if (selectedIds.size === 0) return;
    function onKeyDown(e: KeyboardEvent) {
      const target = e.target;
      const typing =
        target instanceof HTMLElement &&
        (target instanceof HTMLInputElement ||
          target instanceof HTMLTextAreaElement ||
          target.isContentEditable);
      if (typing) return;
      if (e.key === "Escape") clear();
      else if ((e.ctrlKey || e.metaKey) && (e.key === "a" || e.key === "A")) {
        e.preventDefault();
        setSelectedIds(new Set(items.map((item) => item.id)));
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectedIds.size, items, clear]);

  const selected = useMemo(
    () => items.filter((item) => selectedIds.has(item.id)),
    [items, selectedIds],
  );

  return useMemo(
    () => ({
      count: selectedIds.size,
      isSelected: (id: string) => selectedIds.has(id),
      selected,
      onRowClick,
      onRowContextMenu,
      // A row outside the selection acts alone — matching what the click that
      // opened the menu already did to the selection.
      targetsFor: (item: T) => (selectedIds.has(item.id) ? selected : [item]),
      clear,
    }),
    [selectedIds, selected, onRowClick, onRowContextMenu, clear],
  );
}
