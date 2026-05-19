import { open } from '@tauri-apps/plugin-dialog';
import { AlertTriangle, CheckCircle, FileUp } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import * as ipc from '../../../lib/ipc';
import { parseScriptText } from '../../../lib/scriptImporter';
import { useCharacterStore } from '../../../store/characterStore';
import { useProjectStore } from '../../../store/projectStore';
import { useScriptStore } from '../../../store/scriptStore';
import { Alert, AlertTitle } from '../../ui/alert';
import { Button } from '../../ui/button';
import ImportMappingDialog from '../ImportMappingDialog';

import type { ScriptLine, ScriptSection } from '../../../types';
import type { CharacterMapping } from '../ImportMappingDialog';

export default function ImportSection() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);

  const [importOpen, setImportOpen] = useState(false);
  const [importParseResult, setImportParseResult] = useState<ReturnType<typeof parseScriptText> | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importSuccess, setImportSuccess] = useState(false);

  const handleImportSelect = async () => {
    setImportError(null);
    setImportSuccess(false);
    const selected = await open({
      title: t('project.importSelectFile'),
      multiple: false,
      filters: [{ name: 'Text Files', extensions: ['txt'] }],
    });
    if (!selected) return;

    const filePath = Array.isArray(selected)
      ? (selected[0] as string)
      : typeof selected === 'object'
        ? (selected as { filePath: string }).filePath
        : selected;

    try {
      const content = await ipc.readTextFile(filePath);
      const result = parseScriptText(content);
      if (result.lines.length === 0) {
        setImportError(t('project.importNoContent'));
        return;
      }
      setImportParseResult(result);
      setImportOpen(true);
    } catch (e: unknown) {
      setImportError(`${t('project.importParseFailed')}: ${String(e)}`);
    }
  };

  const handleImportConfirm = async (mapping: CharacterMapping[]) => {
    if (!currentProject) return;
    const projectId = currentProject.project.id;

    try {
      const charIdMap = new Map<string, string>();
      for (const m of mapping) {
        if (m.type === 'existing' && m.characterId) {
          charIdMap.set(m.fileCharacterName, m.characterId);
        } else if (m.type === 'new' && m.newCharacterName) {
          const settingsMod = await import('../../../store/settingsStore');
          const settings = settingsMod.useSettingsStore.getState();
          const character = await ipc.createCharacter(projectId, {
            name: m.newCharacterName,
            voice_name: settings.defaultVoiceName,
            tts_model: settings.defaultTtsModel,
            speed: settings.defaultSpeed,
            pitch: settings.defaultPitch,
          });
          charIdMap.set(m.fileCharacterName, character.id);
          await useCharacterStore.getState().fetchCharacters();
        }
      }

      const existingSections = useScriptStore.getState().sections;
      const sectionMap = new Map<string, ScriptSection>();
      let sectionOrder = existingSections.length;

      if (importParseResult) {
        for (const sectionName of importParseResult.sectionNames) {
          const existing = existingSections.find((s) => s.title === sectionName);
          if (existing) {
            sectionMap.set(sectionName, existing);
          } else {
            const newSection: ScriptSection = {
              id: crypto.randomUUID(),
              project_id: projectId,
              title: sectionName,
              section_order: sectionOrder++,
            };
            sectionMap.set(sectionName, newSection);
          }
        }
      }

      const newSections = [
        ...existingSections,
        ...[...sectionMap.values()].filter((s) => !existingSections.some((e) => e.id === s.id)),
      ];

      const existingLines = useScriptStore.getState().lines;
      let lineOrder = existingLines.length;

      const importedLines: ScriptLine[] = (importParseResult?.lines ?? []).map((parsed) => ({
        id: crypto.randomUUID(),
        project_id: projectId,
        line_order: lineOrder++,
        text: parsed.text,
        character_id: parsed.characterName ? (charIdMap.get(parsed.characterName) ?? null) : null,
        gap_after_ms: 500,
        instructions: '',
        section_id: parsed.sectionName ? (sectionMap.get(parsed.sectionName)?.id ?? null) : null,
      }));

      useScriptStore.setState({ lines: [...existingLines, ...importedLines], sections: newSections, isDirty: true });
      await useScriptStore.getState().saveScript();
      await useProjectStore.getState().loadProject(projectId);
      await useCharacterStore.getState().fetchCharacters();

      setImportSuccess(true);
    } catch (e) {
      setImportError(`${t('project.importFailed')}: ${e}`);
    }
  };

  return (
    <div className="mt-10 pt-6 border-t border-border/50">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium">{t('project.importScript')}</h3>
          <p className="text-xs text-muted-foreground mt-0.5">{t('project.importSelectFile')}</p>
        </div>
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5"
          onClick={() => void handleImportSelect().catch(() => {})}
        >
          <FileUp className="h-3.5 w-3.5" />
          {t('project.importScript')}
        </Button>
      </div>

      {importError && (
        <Alert variant="destructive" className="mt-3">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{importError}</AlertTitle>
        </Alert>
      )}

      {importSuccess && (
        <Alert className="mt-3">
          <CheckCircle className="h-4 w-4 text-green-500" />
          <AlertTitle>{t('project.importSuccess')}</AlertTitle>
        </Alert>
      )}

      {importParseResult && (
        <ImportMappingDialog
          open={importOpen}
          onOpenChange={setImportOpen}
          parseResult={importParseResult}
          existingCharacters={currentProject?.characters ?? []}
          onConfirm={(mapping) => void handleImportConfirm(mapping)}
        />
      )}
    </div>
  );
}
