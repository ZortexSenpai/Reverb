// Drag-to-reorder for the queue panel's "Next up" list. The list is
// virtualized (a Queue can hold thousands of Songs and rendering every row made
// the panel take seconds to open), so a library that needs every row in the DOM
// to reorder won't do; with rows of one fixed height, where a row lands is
// plain arithmetic instead.

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

/** Every row in the queue panel is this tall: py-1.5 around a size-10 cover. */
export const ROW_HEIGHT = 52;

/** How far the pointer moves before a press on a row becomes a drag, so clicks still click. */
const DRAG_THRESHOLD = 4;

/** Where a row dragged `dy` px from its slot `from` lands among `count` rows. */
export function dropIndex(from: number, dy: number, count: number): number {
  return Math.max(0, Math.min(count - 1, from + Math.round(dy / ROW_HEIGHT)));
}

/**
 * How many rows the row at `index` steps aside while the row from `from` hangs
 * over `to`: the ones in between shift one place to make room for it.
 */
export function displacement(index: number, from: number, to: number): -1 | 0 | 1 {
  if (from < to && index > from && index <= to) return -1;
  if (to < from && index >= to && index < from) return 1;
  return 0;
}

/**
 * Pixels to scroll per frame with the pointer at `y`, near the top or bottom
 * edge of a scroll area spanning `top`–`bottom`: faster the closer to the edge.
 */
export function edgeScrollSpeed(
  y: number,
  top: number,
  bottom: number,
  zone = 48,
  max = 12,
): number {
  const depth = y < top + zone ? y - (top + zone) : y > bottom - zone ? y - (bottom - zone) : 0;
  return Math.round(max * Math.max(-1, Math.min(1, depth / zone)));
}

export interface QueueDrag {
  queueId: string;
  /** The row's own slot. */
  from: number;
  /** The slot it lands in if dropped now. */
  to: number;
  /** How far it has been dragged from its slot, scrolling included. */
  dy: number;
}

/**
 * Pointer handling for dragging rows of the list in `scrollElement` (scrolling
 * it when held near an edge). `onDrop` gets the slot the row landed in.
 */
export function useQueueDrag(
  scrollElement: HTMLElement | null,
  count: number,
  onDrop: (queueId: string, to: number) => void,
) {
  const [drag, setDrag] = useState<QueueDrag | null>(null);
  // Tears down the drag in progress, should the list unmount mid-drag.
  const stop = useRef<(() => void) | null>(null);
  useEffect(() => () => stop.current?.(), []);

  const onPointerDown = useCallback(
    (e: ReactPointerEvent<HTMLElement>, from: number, queueId: string) => {
      if (e.button !== 0 || !scrollElement) return;
      const row = e.currentTarget;
      const pointerId = e.pointerId;
      const startY = e.clientY;
      const startScroll = scrollElement.scrollTop;
      let pointerY = startY;
      let active = false;
      let frame = 0;
      let lastDy: number | null = null;

      const dragged = () => pointerY - startY + scrollElement.scrollTop - startScroll;

      // Runs every frame while dragging, so holding still at an edge keeps scrolling.
      const tick = () => {
        const { top, bottom } = scrollElement.getBoundingClientRect();
        scrollElement.scrollTop += edgeScrollSpeed(pointerY, top, bottom);
        const dy = dragged();
        if (dy !== lastDy) {
          lastDy = dy;
          setDrag({ queueId, from, to: dropIndex(from, dy, count), dy });
        }
        frame = requestAnimationFrame(tick);
      };

      const move = (ev: PointerEvent) => {
        if (ev.pointerId !== pointerId) return;
        pointerY = ev.clientY;
        if (active || Math.abs(pointerY - startY) < DRAG_THRESHOLD) return;
        active = true;
        // Only now, not on press: captured from the start, a plain click on
        // the row's buttons would land on the row instead.
        row.setPointerCapture(pointerId);
        frame = requestAnimationFrame(tick);
      };

      const teardown = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        window.removeEventListener("pointercancel", teardown);
        cancelAnimationFrame(frame);
        stop.current = null;
        setDrag(null);
      };

      const up = (ev: PointerEvent) => {
        if (ev.pointerId !== pointerId) return;
        teardown();
        if (!active) return;
        swallowNextClick();
        onDrop(queueId, dropIndex(from, dragged(), count));
      };

      stop.current?.();
      stop.current = teardown;
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
      window.addEventListener("pointercancel", teardown);
    },
    [scrollElement, count, onDrop],
  );

  return { drag, onPointerDown };
}

/**
 * The release that ends a drag still clicks whatever it started on (say, the
 * cover's play button).
 */
function swallowNextClick() {
  const swallow = (e: MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
  };
  window.addEventListener("click", swallow, { capture: true, once: true });
  // No click follows when the release lands somewhere else; don't eat a later one.
  setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);
}
