import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { saveGlobalVideoStyle } from '../../lib/ipc';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { useToastStore } from '../../store/toastStore';
import { Label } from '../ui/label';

/**
 * Global video style configuration component.
 * Allows users to set a global style prompt that applies to all section videos by default.
 */
export function GlobalStyleConfig() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);
  const globalVideoStyle = useScriptStore((s) => s.globalVideoStyle);
  const setGlobalVideoStyle = useScriptStore((s) => s.setGlobalVideoStyle);
  const addToast = useToastStore((s) => s.addToast);

  const [localStyle, setLocalStyle] = useState(globalVideoStyle);
  const [isSaving, setIsSaving] = useState(false);

  // Sync local state with store when project changes
  useEffect(() => {
    setLocalStyle(globalVideoStyle);
  }, [globalVideoStyle]);

  // Auto-save with debounce
  useEffect(() => {
    if (!currentProject) return;

    const saveStyle = async () => {
      if (localStyle !== globalVideoStyle && !isSaving) {
        setIsSaving(true);
        try {
          await saveGlobalVideoStyle(currentProject.project.id, localStyle);
          setGlobalVideoStyle(localStyle);
          addToast(t('editor.globalStyleSaved'), 'success');
        } catch (error) {
          console.error('Failed to save global video style:', error);
          addToast(t('editor.globalStyleSaveFailed'), 'error');
        } finally {
          setIsSaving(false);
        }
      }
    };

    const timeoutId = setTimeout(() => {
      void saveStyle();
    }, 500); // 500ms debounce

    return () => {
      clearTimeout(timeoutId);
    };
  }, [localStyle, globalVideoStyle, currentProject, isSaving, setGlobalVideoStyle, addToast, t]);

  return (
    <div className="rounded-lg border border-border/60 bg-card/50 p-4 space-y-3">
      <div className="space-y-1">
        <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          {t('editor.globalVideoStyle')}
        </Label>
        <p className="text-xs text-muted-foreground">{t('editor.globalVideoStyleDescription')}</p>
      </div>
      <textarea
        value={localStyle}
        onChange={(e) => {
          setLocalStyle(e.target.value);
        }}
        placeholder={t('editor.globalVideoStylePlaceholder')}
        className="w-full min-h-20 rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
      />
      {isSaving && <p className="text-xs text-muted-foreground">{t('editor.saving')}</p>}
    </div>
  );
}
