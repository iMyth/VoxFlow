import { Plus, X } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '../ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';

import type { ParseResult } from '../../lib/scriptImporter';
import type { Character } from '../../types';

/** Mapping result: file character name → project character */
export interface CharacterMapping {
  /** Character name from the imported file */
  fileCharacterName: string;
  /** Either an existing character ID or a new character to create */
  type: 'existing' | 'new';
  /** Existing character ID (when type === 'existing') */
  characterId?: string;
  /** New character name (when type === 'new') */
  newCharacterName?: string;
}

interface ImportMappingDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  parseResult: ParseResult;
  existingCharacters: Character[];
  onConfirm: (mapping: CharacterMapping[]) => void;
}

const CREATE_NEW = '__create_new__';
const UNMAPPED = '__unassigned__';

export default function ImportMappingDialog({
  open,
  onOpenChange,
  parseResult,
  existingCharacters,
  onConfirm,
}: ImportMappingDialogProps) {
  const { t } = useTranslation();

  // mapping state: fileCharacterName → { type, value }
  const [mapping, setMapping] = useState<Map<string, { type: 'existing' | 'new'; value: string }>>(new Map());
  // inline new-character name inputs
  const [newCharNames, setNewCharNames] = useState<Map<string, string>>(new Map());

  // Reset state when dialog opens
  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen) {
      setMapping(new Map());
      setNewCharNames(new Map());
    }
    onOpenChange(newOpen);
  };

  const handleSelectChange = (charName: string, value: string) => {
    setMapping((prev) => {
      const next = new Map(prev);
      if (value === UNMAPPED) {
        next.delete(charName);
      } else if (value === CREATE_NEW) {
        next.set(charName, { type: 'new', value: '' });
      } else {
        next.set(charName, { type: 'existing', value });
      }
      return next;
    });
  };

  const handleNewCharNameChange = (charName: string, name: string) => {
    setNewCharNames((prev) => {
      const next = new Map(prev);
      next.set(charName, name);
      return next;
    });
    // Update mapping value to track the name
    setMapping((prev) => {
      const next = new Map(prev);
      const entry = next.get(charName);
      if (entry && entry.type === 'new') {
        next.set(charName, { type: 'new', value: name });
      }
      return next;
    });
  };

  const removeMapping = (charName: string) => {
    setMapping((prev) => {
      const next = new Map(prev);
      next.delete(charName);
      return next;
    });
    setNewCharNames((prev) => {
      const next = new Map(prev);
      next.delete(charName);
      return next;
    });
  };

  // Check if all characters are mapped
  const allMapped =
    parseResult.characterNames.length > 0 &&
    parseResult.characterNames.every((cn) => {
      const entry = mapping.get(cn);
      if (!entry) return false;
      if (entry.type === 'existing') return !!entry.value;
      if (entry.type === 'new') return entry.value.trim().length > 0;
      return false;
    });

  // Also count lines with no character — they don't need mapping
  const unmappedCount = parseResult.lines.filter((l) => l.characterName === null).length;

  const handleConfirm = () => {
    const result: CharacterMapping[] = [];
    for (const [charName, entry] of mapping) {
      if (entry.type === 'existing') {
        result.push({ fileCharacterName: charName, type: 'existing', characterId: entry.value });
      } else {
        const name = newCharNames.get(charName) ?? entry.value;
        result.push({ fileCharacterName: charName, type: 'new', newCharacterName: name.trim() || charName });
      }
    }
    onConfirm(result);
    handleOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg" showCloseButton>
        <DialogHeader>
          <DialogTitle>{t('project.importScriptTitle')}</DialogTitle>
        </DialogHeader>

        <DialogDescription className="space-y-4">
          {/* Stats */}
          <div className="flex gap-4 text-xs text-muted-foreground">
            <span>
              {parseResult.lines.length} {t('project.importLines')}
            </span>
            <span>
              {parseResult.sectionNames.length} {t('project.importSections')}
            </span>
            <span>
              {parseResult.characterNames.length} {t('project.importCharacters')}
            </span>
            {unmappedCount > 0 && (
              <span className="text-amber-600 dark:text-amber-400">
                +{unmappedCount} {t('editor.unassigned')}
              </span>
            )}
          </div>

          {/* Character mapping */}
          {parseResult.characterNames.length > 0 && (
            <div className="space-y-3">
              <Label>{t('project.importCharacterMapping')}</Label>
              <p className="text-xs text-muted-foreground">{t('project.importMapHint')}</p>
              <div className="space-y-2 max-h-60 overflow-y-auto">
                {parseResult.characterNames.map((charName) => {
                  const entry = mapping.get(charName);
                  const isNew = entry?.type === 'new';
                  return (
                    <div key={charName} className="flex items-center gap-2">
                      <span className="text-sm font-medium shrink-0 w-24 truncate" title={charName}>
                        {charName}
                      </span>
                      <span className="text-muted-foreground shrink-0">→</span>
                      {isNew ? (
                        <div className="flex items-center gap-1 flex-1">
                          <Input
                            className="flex-1 h-7 text-sm"
                            placeholder={t('project.importNewCharacterName')}
                            value={newCharNames.get(charName) ?? ''}
                            onChange={(e) => {
                              handleNewCharNameChange(charName, e.target.value);
                            }}
                          />
                          <Button
                            variant="ghost"
                            size="icon-xs"
                            className="shrink-0"
                            onClick={() => {
                              removeMapping(charName);
                            }}
                          >
                            <X className="h-3 w-3" />
                          </Button>
                        </div>
                      ) : (
                        <div className="flex items-center gap-1 flex-1">
                          <Select
                            value={entry?.type === 'existing' ? entry.value : UNMAPPED}
                            onValueChange={(v) => {
                              handleSelectChange(charName, v);
                            }}
                          >
                            <SelectTrigger size="sm" className="flex-1">
                              <SelectValue placeholder={t('project.importUnmapped')} />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectItem value={UNMAPPED}>{t('project.importUnmapped')}</SelectItem>
                              {existingCharacters.map((c) => (
                                <SelectItem key={c.id} value={c.id}>
                                  {c.name}
                                </SelectItem>
                              ))}
                              <SelectItem value={CREATE_NEW}>
                                <Plus className="h-3 w-3 inline" /> {t('project.importCreateNew')}
                              </SelectItem>
                            </SelectContent>
                          </Select>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Section preview */}
          {parseResult.sectionNames.length > 0 && (
            <div className="space-y-1">
              <Label>{t('project.importPreview')}</Label>
              <div className="text-xs text-muted-foreground space-y-0.5 max-h-32 overflow-y-auto">
                {parseResult.sectionNames.map((s) => (
                  <div key={s} className="flex items-center gap-2">
                    <span className="text-foreground font-medium">=== {s} ===</span>
                    <span className="opacity-60">
                      ({parseResult.lines.filter((l) => l.sectionName === s).length} {t('project.importLines')})
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </DialogDescription>

        <DialogFooter className="flex flex-col-reverse sm:flex-row sm:justify-between gap-2">
          <Button
            variant="outline"
            onClick={() => {
              handleOpenChange(false);
            }}
          >
            {t('project.importCancel')}
          </Button>
          <Button onClick={handleConfirm} disabled={!allMapped}>
            {t('project.importConfirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
