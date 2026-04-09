import {
  Plus,
  Undo2,
  Redo2,
  Wand2,
  X,
  Volume2,
  RefreshCw,
  Save,
  Sparkles,
} from "lucide-react";
import { useState, useEffect, useRef, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { Progress } from "../ui/progress";
import { Input } from "../ui/input";
import { Separator } from "../ui/separator";
import ScriptLineComponent from "./ScriptLine";
import SectionGroup from "./SectionGroup";
import ModeSelector from "./ModeSelector";
import type { ScriptLine, ScriptSection } from "../../types";
import { useScriptStore } from "../../store/scriptStore";

interface ScriptLinesProps {
  lines: ScriptLine[];
  sections: ScriptSection[];
  emptyHint: string;
  showOutlineBtn?: boolean;
  onEditOutline?: () => void;
  workflow: "ai" | "manual" | null;
  onSelectAi: () => void;
  onSelectManual: () => void;
  isDirty?: boolean;
  isBatchTtsRunning?: boolean;
  batchTtsProgress?: { current: number; total: number } | null;
  missingTtsCount?: number;
  hasAudioCount?: number;
  onSave?: () => void;
  onGenerateAllTts?: () => void;
  onRegenerateAllTts?: () => void;
}

export default function ScriptLines({
  lines,
  sections,
  emptyHint,
  showOutlineBtn,
  onEditOutline,
  workflow,
  onSelectAi,
  onSelectManual,
  isDirty,
  isBatchTtsRunning,
  batchTtsProgress,
  missingTtsCount,
  hasAudioCount,
  onSave,
  onGenerateAllTts,
  onRegenerateAllTts,
}: ScriptLinesProps) {
  const { t } = useTranslation();
  const { addLine, addSection, setAllInstructions, reorderLines } = useScriptStore();
  const [batchInstructionsOpen, setBatchInstructionsOpen] = useState(false);
  const [batchInstructionsValue, setBatchInstructionsValue] = useState("");
  const [outlineBtnBouncing, setOutlineBtnBouncing] = useState(false);
  const outlineBtnAnimated = useRef(false);

  // Drag state for flat line list
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);

  const handleDragStart = useCallback((lineId: string) => {
    setDraggingId(lineId);
    setDropTargetId(null);
  }, []);

  const handleDragMove = useCallback((clientX: number, clientY: number) => {
    if (!draggingId) return;
    const el = document.elementFromPoint(clientX, clientY);
    const card = el?.closest('[data-line-id]');
    const targetId = card?.getAttribute('data-line-id') ?? null;
    setDropTargetId(prev => prev !== targetId ? targetId : prev);
  }, [draggingId]);

  const handleDragEnd = useCallback(() => {
    if (draggingId && dropTargetId && draggingId !== dropTargetId) {
      const allLines = useScriptStore.getState().lines;
      const fromIdx = allLines.findIndex((l) => l.id === draggingId);
      const toIdx = allLines.findIndex((l) => l.id === dropTargetId);
      if (fromIdx !== -1 && toIdx !== -1) {
        reorderLines(fromIdx, toIdx);
      }
    }
    setDraggingId(null);
    setDropTargetId(null);
  }, [draggingId, dropTargetId, reorderLines]);

  // Trigger bounce animation on first mount when outline button is visible
  useEffect(() => {
    if (showOutlineBtn && !outlineBtnAnimated.current) {
      outlineBtnAnimated.current = true;
      setOutlineBtnBouncing(true);
      const timer = setTimeout(() => setOutlineBtnBouncing(false), 1000);
      return () => clearTimeout(timer);
    }
  }, [showOutlineBtn]);

  const handleBatchInstructions = () => {
    if (batchInstructionsValue.trim()) {
      setAllInstructions(batchInstructionsValue.trim());
      setBatchInstructionsOpen(false);
      setBatchInstructionsValue("");
    }
  };

  const handleBatchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleBatchInstructions();
    } else if (e.key === "Escape") {
      setBatchInstructionsOpen(false);
      setBatchInstructionsValue("");
    }
  };

  const hasLines = lines.length > 0;

  const Toolbar = (
    <div className="flex items-center justify-between mb-1">
      <div className="flex items-center gap-1">
        {showOutlineBtn && onEditOutline && (
          <Button
            variant="outline"
            size="sm"
            className="h-7 px-3 text-xs gap-1.5 border-blue-300 dark:border-blue-700 text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20"
            onClick={onEditOutline}
            style={outlineBtnBouncing
              ? { animation: "bounce-once 0.8s ease 1" }
              : undefined}
          >
            <Sparkles className="h-3.5 w-3.5" />
            {t("editor.editOutline")}
          </Button>
        )}
        {hasLines && (
          <>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 w-6 p-0"
              onClick={() => useScriptStore.temporal.getState().undo()}
              title={`${t("editor.undo")} (⌘Z)`}
            >
              <Undo2 className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 w-6 p-0"
              onClick={() => useScriptStore.temporal.getState().redo()}
              title={`${t("editor.redo")} (⇧⌘Z)`}
            >
              <Redo2 className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className={`h-6 w-6 p-0 ${batchInstructionsOpen ? "text-purple-500" : ""}`}
              onClick={() => {
                if (batchInstructionsOpen) {
                  setBatchInstructionsOpen(false);
                  setBatchInstructionsValue("");
                } else {
                  setBatchInstructionsOpen(true);
                }
              }}
              title={t("editor.setAllInstructions")}
            >
              {batchInstructionsOpen ? (
                <X className="h-3.5 w-3.5" />
              ) : (
                <Wand2 className="h-3.5 w-3.5" />
              )}
            </Button>
            {batchInstructionsOpen && (
              <div className="flex items-center gap-2 flex-1 ml-2">
                <Input
                  value={batchInstructionsValue}
                  onChange={(e) => setBatchInstructionsValue(e.target.value)}
                  onKeyDown={handleBatchKeyDown}
                  className="h-7 text-xs flex-1 border-purple-300/50 focus-visible:border-purple-500"
                  placeholder={t("editor.instructionsPlaceholder")}
                  autoFocus
                />
                <Button
                  size="sm"
                  className="h-7 text-xs"
                  onClick={handleBatchInstructions}
                  disabled={!batchInstructionsValue.trim()}
                >
                  {t("editor.setAllInstructions")}
                </Button>
              </div>
            )}
          </>
        )}
      </div>
      <div className="flex items-center gap-2">
        {isDirty && (
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs gap-1"
            onClick={onSave}
          >
            <Save className="h-3.5 w-3.5" />
            {t("editor.save")}
          </Button>
        )}
        {(missingTtsCount ?? 0) > 0 && onGenerateAllTts && (
          <Button
            variant="outline"
            size="sm"
            className="h-6 px-2 text-xs gap-1 border-blue-300 dark:border-blue-700 text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20"
            onClick={onGenerateAllTts}
            disabled={isBatchTtsRunning}
          >
            <Volume2 className="h-3.5 w-3.5" />
            {isBatchTtsRunning
              ? t("editor.batchTtsRunning", {
                  current: batchTtsProgress?.current ?? 0,
                  total: batchTtsProgress?.total ?? missingTtsCount,
                })
              : t("editor.generateAllTts", { count: missingTtsCount })}
          </Button>
        )}
        {(missingTtsCount ?? 0) === 0 && (hasAudioCount ?? 0) > 0 && onRegenerateAllTts && (
          <Button
            variant="outline"
            size="sm"
            className="h-6 px-2 text-xs gap-1 border-orange-300 dark:border-orange-700 text-orange-600 dark:text-orange-400 hover:bg-orange-50 dark:hover:bg-orange-900/20"
            onClick={onRegenerateAllTts}
            disabled={isBatchTtsRunning}
          >
            <RefreshCw className="h-3.5 w-3.5" />
            {isBatchTtsRunning
              ? t("editor.batchTtsRunning", {
                  current: batchTtsProgress?.current ?? 0,
                  total: batchTtsProgress?.total ?? hasAudioCount,
                })
              : t("editor.regenerateAllTts", { count: hasAudioCount })}
          </Button>
        )}
        <ModeSelector
          onSelectAi={onSelectAi}
          onSelectManual={onSelectManual}
          currentMode={workflow ?? null}
        />
      </div>
      {isBatchTtsRunning && batchTtsProgress && (
        <div className="flex-1 min-w-[120px] space-y-1">
          <Progress
            value={(batchTtsProgress.current / batchTtsProgress.total) * 100}
            className="h-2"
          />
          <p className="text-xs text-muted-foreground">
            {batchTtsProgress.current} / {batchTtsProgress.total}
          </p>
        </div>
      )}
    </div>
  );

  const sortedSections = [...sections].sort(
    (a, b) => a.section_order - b.section_order,
  );

  // If we have sections, render section groups
  if (sortedSections.length > 0) {
    // Build a map of section_id -> lines
    const linesBySection = new Map<string, ScriptLine[]>();
    const unassignedLines: ScriptLine[] = [];

    for (const line of lines) {
      if (line.section_id && linesBySection.has(line.section_id)) {
        linesBySection.get(line.section_id)!.push(line);
      } else if (line.section_id) {
        linesBySection.set(line.section_id, [line]);
      } else {
        unassignedLines.push(line);
      }
    }

    return (
      <>
        {Toolbar}
        <div className="space-y-4">
          {sortedSections.map((section, index) => (
            <SectionGroup
              key={section.id}
              section={section}
              lines={linesBySection.get(section.id) ?? []}
              index={index}
              totalSections={sortedSections.length}
              onAddLine={() => addLine(-1, section.id)}
            />
          ))}
          {/* Unassigned lines */}
          {unassignedLines.length > 0 && (
            <div className="space-y-2">
              {unassignedLines.map((line, index) => (
                <ScriptLineComponent
                  key={line.id}
                  line={line}
                  index={index}
                  isDragging={draggingId === line.id}
                  isDropTarget={dropTargetId === line.id}
                  onDragStart={handleDragStart}
                  onDragMove={handleDragMove}
                  onDragEnd={handleDragEnd}
                />
              ))}
            </div>
          )}
          <Button
            variant="outline"
            className="w-full border-dashed"
            onClick={addSection}
          >
            <Plus className="h-4 w-4" /> {t("editor.addSection")}
          </Button>
        </div>
      </>
    );
  }

  // No sections: flat list (backward compatible, fallback section "正文")
  return (
    <>
      <Separator className="border-dashed" />
      {Toolbar}
      <div className="space-y-2">
        {lines.length === 0 && (
          <p className="text-center text-muted-foreground py-8">{emptyHint}</p>
        )}
        {lines.map((line, index) => (
          <ScriptLineComponent
            key={line.id}
            line={line}
            index={index}
            isDragging={draggingId === line.id}
            isDropTarget={dropTargetId === line.id}
            onDragStart={handleDragStart}
            onDragMove={handleDragMove}
            onDragEnd={handleDragEnd}
          />
        ))}
        <Button
          variant="outline"
          className="w-full border-dashed"
          onClick={() => addLine(lines.length - 1)}
        >
          <Plus className="h-4 w-4" /> {t("editor.addLine")}
        </Button>
      </div>
    </>
  );
}
