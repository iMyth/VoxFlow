import { useState, useCallback } from 'react';

interface UseDragAndDropOptions {
  /** Called with (fromIndex, toIndex) when a drop is completed */
  onReorder?: (fromIdx: number, toIdx: number) => void;
  /** Optional: getter for the current list of all lines (needed to find indices) */
  getLines?: () => Array<{ id: string }>;
  /** Optional: direct reorder function */
  reorderFn?: (fromIdx: number, toIdx: number) => void;
}

interface UseDragAndDropReturn {
  draggingId: string | null;
  dropTarget: { id: string; position: 'before' | 'after' } | null;
  handleDragStart: (lineId: string) => void;
  handleDragMove: (clientX: number, clientY: number) => void;
  handleDragEnd: () => void;
}

export function useDragAndDrop({ onReorder, getLines, reorderFn }: UseDragAndDropOptions = {}): UseDragAndDropReturn {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<{ id: string; position: 'before' | 'after' } | null>(null);

  const handleDragStart = useCallback((lineId: string, _pointerId?: number) => {
    setDraggingId(lineId);
    setDropTarget(null);
  }, []);

  const handleDragMove = useCallback(
    (clientX: number, clientY: number) => {
      if (!draggingId) return;
      const el = document.elementFromPoint(clientX, clientY);
      const card = el?.closest('[data-line-id]') as HTMLElement | null;
      if (!card) {
        setDropTarget(null);
        return;
      }
      const targetId = card.getAttribute('data-line-id');
      if (!targetId || targetId === draggingId) {
        setDropTarget(null);
        return;
      }
      const rect = card.getBoundingClientRect();
      const position: 'before' | 'after' = clientY < rect.top + rect.height / 2 ? 'before' : 'after';
      setDropTarget((prev) =>
        prev && prev.id === targetId && prev.position === position ? prev : { id: targetId, position }
      );
    },
    [draggingId]
  );

  const handleDragEnd = useCallback(() => {
    if (draggingId && dropTarget && draggingId !== dropTarget.id) {
      const allLines = getLines?.() ?? [];
      const fromIdx = allLines.findIndex((l) => l.id === draggingId);
      const targetIdx = allLines.findIndex((l) => l.id === dropTarget.id);
      if (fromIdx !== -1 && targetIdx !== -1) {
        let toIdx = dropTarget.position === 'before' ? targetIdx : targetIdx + 1;
        if (fromIdx < toIdx) toIdx -= 1;
        if (fromIdx !== toIdx) {
          if (reorderFn) {
            reorderFn(fromIdx, toIdx);
          } else if (onReorder) {
            onReorder(fromIdx, toIdx);
          }
        }
      }
    }
    setDraggingId(null);
    setDropTarget(null);
  }, [draggingId, dropTarget, getLines, onReorder, reorderFn]);

  return {
    draggingId,
    dropTarget,
    handleDragStart,
    handleDragMove,
    handleDragEnd,
  };
}
