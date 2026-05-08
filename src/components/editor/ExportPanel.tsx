import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { save, open } from '@tauri-apps/plugin-dialog';
import {
  Download,
  Music,
  AlertTriangle,
  CheckCircle,
  Loader2,
  FolderOpen,
  Play,
  Pause,
  FileUp,
  Video,
  Image,
} from 'lucide-react';
import { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import ImportMappingDialog from './ImportMappingDialog';
import * as ipc from '../../lib/ipc';
import { parseScriptText } from '../../lib/scriptImporter';
import { useCharacterStore } from '../../store/characterStore';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { Alert, AlertTitle, AlertDescription } from '../ui/alert';
import { Button } from '../ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Progress } from '../ui/progress';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Slider } from '../ui/slider';

import type { CharacterMapping } from './ImportMappingDialog';
import type { VideoStyle } from '../../lib/ipc';
import type { MixProgress, ScriptLine, ScriptSection } from '../../types';

export default function ExportPanel() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);
  const { lines } = useScriptStore();
  const [bgmPath, setBgmPath] = useState<string | null>(null);
  const [bgmVolume, setBgmVolume] = useState(0.3);
  const [bgmPlaying, setBgmPlaying] = useState(false);
  const [outputPath, setOutputPath] = useState('');
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState<MixProgress | null>(null);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Import state
  const [importOpen, setImportOpen] = useState(false);
  const [importParseResult, setImportParseResult] = useState<ReturnType<typeof parseScriptText> | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importSuccess, setImportSuccess] = useState(false);

  // Video export state
  const [videoStyle, setVideoStyle] = useState<VideoStyle>('particles');
  const [videoFgColor, setVideoFgColor] = useState('6366f1');
  const [videoBgColor, setVideoBgColor] = useState('1a1a2e');
  const [videoBgImage, setVideoBgImage] = useState<string | null>(null);
  const [videoExporting, setVideoExporting] = useState(false);
  const [videoProgress, setVideoProgress] = useState<MixProgress | null>(null);
  const [videoDone, setVideoDone] = useState(false);
  const [videoError, setVideoError] = useState<string | null>(null);
  const [lastExportedAudioPath, setLastExportedAudioPath] = useState<string | null>(null);

  const audioFragments = currentProject?.audio_fragments;
  const coveredLineIds = useMemo(() => new Set((audioFragments ?? []).map((a) => a.line_id)), [audioFragments]);
  const missingLines = useMemo(
    () => lines.filter((l) => l.text.trim() && !coveredLineIds.has(l.id)),
    [lines, coveredLineIds]
  );

  useEffect(() => {
    if (currentProject) {
      setOutputPath(`${currentProject.project.name}.mp3`);
    }
  }, [currentProject]);

  // Stop BGM preview on unmount and listen for audio-finished
  useEffect(() => {
    const unlisten = listen('audio-finished', () => {
      setBgmPlaying(false);
    });
    return () => {
      void unlisten.then((fn) => {
        fn();
      });
      if (bgmPlaying) {
        invoke('stop_audio').catch(() => {});
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleBgmBrowse = async () => {
    const selected = await open({
      title: t('export.selectBgm'),
      multiple: false,
      filters: [{ name: 'Audio Files', extensions: ['mp3', 'wav', 'flac', 'ogg', 'm4a', 'aac'] }],
    });
    if (selected) {
      // eslint-disable-next-line @typescript-eslint/no-unsafe-argument
      setBgmPath(Array.isArray(selected) ? selected[0] : selected);
    }
  };

  const toggleBgmPreview = async () => {
    if (!bgmPath) return;
    try {
      if (bgmPlaying) {
        await invoke('stop_audio');
        setBgmPlaying(false);
      } else {
        await invoke('play_audio', { filePath: bgmPath });
        setBgmPlaying(true);
      }
    } catch {
      setBgmPlaying(false);
    }
  };

  const handleBgmVolumeChange = async (value: number[]) => {
    const vol = value[0];
    setBgmVolume(vol);
    if (bgmPlaying) {
      try {
        await invoke('set_audio_volume', { volume: vol });
      } catch {
        // ignore
      }
    }
  };

  const handleExport = async () => {
    if (!currentProject || missingLines.length > 0) return;

    // Open save dialog to let user choose full output path
    const selectedPath = await save({
      title: t('editor.exportAudiobookTitle'),
      defaultPath: outputPath,
      filters: [{ name: 'MP3 Audio', extensions: ['mp3'] }],
    });
    if (!selectedPath) return;

    setExporting(true);
    setProgress(null);
    setDone(false);
    setError(null);

    const unlisten = await ipc.onMixProgress((p) => {
      setProgress(p);
    });

    try {
      await ipc.exportAudioMix(currentProject.project.id, selectedPath, bgmPath, bgmVolume);
      setDone(true);
      setLastExportedAudioPath(selectedPath);
    } catch (e) {
      setError(String(e));
    } finally {
      unlisten();
      setExporting(false);
    }
  };

  // ---- Import handlers ----

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

  const handleVideoBgImageBrowse = async () => {
    const selected = await open({
      title: t('export.selectBgImage'),
      multiple: false,
      filters: [{ name: 'Image Files', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp'] }],
    });
    if (selected) {
      // eslint-disable-next-line @typescript-eslint/no-unsafe-argument
      setVideoBgImage(Array.isArray(selected) ? selected[0] : selected);
    }
  };

  const handleExportVideo = async () => {
    if (!lastExportedAudioPath) return;

    const selectedPath = await save({
      title: t('export.exportVideoTitle'),
      defaultPath: `${currentProject?.project.name ?? 'output'}.mp4`,
      filters: [{ name: 'MP4 Video', extensions: ['mp4'] }],
    });
    if (!selectedPath) return;

    setVideoExporting(true);
    setVideoProgress(null);
    setVideoDone(false);
    setVideoError(null);

    const unlisten = await ipc.onVideoProgress((p) => {
      setVideoProgress(p);
    });

    try {
      await ipc.exportVideo({
        audio_path: lastExportedAudioPath,
        output_path: selectedPath,
        style: videoStyle,
        width: 1920,
        height: 1080,
        fg_color: videoFgColor,
        bg_color: videoBgColor,
        bg_image_path: videoBgImage,
        fps: 30,
      });
      setVideoDone(true);
    } catch (e) {
      setVideoError(String(e));
    } finally {
      unlisten();
      setVideoExporting(false);
    }
  };

  const handleImportConfirm = async (mapping: CharacterMapping[]) => {
    if (!currentProject) return;
    const projectId = currentProject.project.id;

    try {
      // 1. Create new characters if any
      const charIdMap = new Map<string, string>(); // fileCharacterName → characterId
      for (const m of mapping) {
        if (m.type === 'existing' && m.characterId) {
          charIdMap.set(m.fileCharacterName, m.characterId);
        } else if (m.type === 'new' && m.newCharacterName) {
          const settingsMod = await import('../../store/settingsStore');
          const settings = settingsMod.useSettingsStore.getState();
          const character = await ipc.createCharacter(projectId, {
            name: m.newCharacterName,
            voice_name: settings.defaultVoiceName,
            tts_model: settings.defaultTtsModel,
            speed: settings.defaultSpeed,
            pitch: settings.defaultPitch,
          });
          charIdMap.set(m.fileCharacterName, character.id);
          // Also update local character store
          await useCharacterStore.getState().fetchCharacters();
        }
      }

      // 2. Build sections
      const existingSections = useScriptStore.getState().sections;
      const sectionMap = new Map<string, ScriptSection>(); // sectionName → section
      let sectionOrder = existingSections.length;

      if (importParseResult) {
        for (const sectionName of importParseResult.sectionNames) {
          // Try to match existing section by name
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

      // 3. Build script lines
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

      // 4. Set into store and save
      useScriptStore.setState({ lines: [...existingLines, ...importedLines], sections: newSections, isDirty: true });
      await useScriptStore.getState().saveScript();

      // 5. Reload project to sync
      await useProjectStore.getState().loadProject(projectId);
      await useCharacterStore.getState().fetchCharacters();

      setImportSuccess(true);
    } catch (e) {
      setImportError(`${t('project.importFailed')}: ${e}`);
    }
  };

  return (
    <div className="px-6 py-8 space-y-6">
      <h2 className="text-xl font-bold">{t('export.title')}</h2>

      {missingLines.length > 0 && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('export.missingAudio', { count: missingLines.length })}</AlertTitle>
          <AlertDescription>
            <ul className="mt-1 space-y-0.5">
              {missingLines.slice(0, 5).map((l) => (
                <li key={l.id}>{t('export.missingLine', { line: l.line_order + 1, text: l.text.slice(0, 40) })}</li>
              ))}
              {missingLines.length > 5 && <li>{t('export.missingMore', { count: missingLines.length - 5 })}</li>}
            </ul>
            <p className="mt-2">{t('export.missingHint')}</p>
          </AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Music className="h-4 w-4" /> {t('export.bgm')}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex gap-2 items-center">
            <Input
              className="flex-1"
              placeholder={t('export.bgmPlaceholder')}
              value={bgmPath ?? ''}
              onChange={(e) => {
                setBgmPath(e.target.value || null);
              }}
            />
            <Button variant="outline" size="icon" onClick={() => void handleBgmBrowse()} title={t('export.browse')}>
              <FolderOpen className="h-4 w-4" />
            </Button>
            {bgmPath && (
              <Button
                variant="outline"
                size="icon"
                onClick={() => void toggleBgmPreview()}
                title={bgmPlaying ? t('editor.pause') : t('editor.play')}
              >
                {bgmPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
              </Button>
            )}
          </div>
          {bgmPath && (
            <div className="space-y-2">
              <Label>{t('export.bgmVolume', { percent: Math.round(bgmVolume * 100) })}</Label>
              <Slider
                min={0}
                max={1}
                step={0.05}
                value={[bgmVolume]}
                onValueChange={(v) => void handleBgmVolumeChange(v)}
              />
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardContent className="space-y-2">
          <Label>{t('export.outputLabel')}</Label>
          <Input
            value={outputPath}
            onChange={(e) => {
              setOutputPath(e.target.value);
            }}
          />
        </CardContent>
      </Card>

      {exporting && progress && (
        <Alert>
          <Loader2 className="h-4 w-4 animate-spin" />
          <AlertTitle>{progress.stage}</AlertTitle>
          <AlertDescription className="space-y-2">
            <Progress value={progress.percent} className="h-2" />
            <p className="text-xs">{Math.round(progress.percent)}%</p>
          </AlertDescription>
        </Alert>
      )}

      {done && (
        <Alert>
          <CheckCircle className="h-4 w-4 text-green-500" />
          <AlertTitle>{t('export.exportSuccess')}</AlertTitle>
        </Alert>
      )}

      {error && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('export.exportFailed')}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {importError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{importError}</AlertTitle>
        </Alert>
      )}

      {importSuccess && (
        <Alert>
          <CheckCircle className="h-4 w-4 text-green-500" />
          <AlertTitle>{t('project.importSuccess')}</AlertTitle>
        </Alert>
      )}

      <Button
        size="lg"
        onClick={() => void handleExport()}
        disabled={exporting || missingLines.length > 0 || !outputPath.trim()}
      >
        <Download className="h-4 w-4" />
        {exporting ? t('export.exporting') : t('export.exportButton')}
      </Button>

      {/* Video Export Section */}
      <div className="pt-4 border-t">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Video className="h-4 w-4" /> {t('export.videoTitle')}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {!lastExportedAudioPath && (
              <p className="text-sm text-muted-foreground">{t('export.videoNeedAudioFirst')}</p>
            )}

            {lastExportedAudioPath && (
              <>
                <div className="space-y-2">
                  <Label>{t('export.videoStyle')}</Label>
                  <Select
                    value={videoStyle}
                    onValueChange={(v) => {
                      setVideoStyle(v as VideoStyle);
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="particles">{t('export.styleParticles')}</SelectItem>
                      <SelectItem value="showwaves">{t('export.styleShowwaves')}</SelectItem>
                      <SelectItem value="showfreqs">{t('export.styleShowfreqs')}</SelectItem>
                      <SelectItem value="avectorscope">{t('export.styleAvectorscope')}</SelectItem>
                      <SelectItem value="showspectrum">{t('export.styleShowspectrum')}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label>{t('export.videoFgColor')}</Label>
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground">#</span>
                      <Input
                        value={videoFgColor}
                        onChange={(e) => {
                          setVideoFgColor(e.target.value.replace(/[^0-9a-fA-F]/g, '').slice(0, 6));
                        }}
                        className="flex-1"
                        maxLength={6}
                      />
                      <div className="w-8 h-8 rounded border" style={{ backgroundColor: `#${videoFgColor}` }} />
                    </div>
                  </div>
                  <div className="space-y-2">
                    <Label>{t('export.videoBgColor')}</Label>
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground">#</span>
                      <Input
                        value={videoBgColor}
                        onChange={(e) => {
                          setVideoBgColor(e.target.value.replace(/[^0-9a-fA-F]/g, '').slice(0, 6));
                        }}
                        className="flex-1"
                        maxLength={6}
                      />
                      <div className="w-8 h-8 rounded border" style={{ backgroundColor: `#${videoBgColor}` }} />
                    </div>
                  </div>
                </div>

                <div className="space-y-2">
                  <Label>{t('export.videoBgImage')}</Label>
                  <div className="flex gap-2 items-center">
                    <Input
                      className="flex-1"
                      placeholder={t('export.videoBgImagePlaceholder')}
                      value={videoBgImage ?? ''}
                      readOnly
                    />
                    <Button variant="outline" size="icon" onClick={() => void handleVideoBgImageBrowse()}>
                      <Image className="h-4 w-4" />
                    </Button>
                    {videoBgImage && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => {
                          setVideoBgImage(null);
                        }}
                      >
                        ✕
                      </Button>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground">{t('export.videoBgImageHint')}</p>
                </div>

                {videoExporting && videoProgress && (
                  <Alert>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    <AlertTitle>{videoProgress.stage}</AlertTitle>
                    <AlertDescription className="space-y-2">
                      <Progress value={videoProgress.percent} className="h-2" />
                      <p className="text-xs">{Math.round(videoProgress.percent)}%</p>
                    </AlertDescription>
                  </Alert>
                )}

                {videoDone && (
                  <Alert>
                    <CheckCircle className="h-4 w-4 text-green-500" />
                    <AlertTitle>{t('export.videoExportSuccess')}</AlertTitle>
                  </Alert>
                )}

                {videoError && (
                  <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertTitle>{t('export.videoExportFailed')}</AlertTitle>
                    <AlertDescription>{videoError}</AlertDescription>
                  </Alert>
                )}

                <Button
                  size="lg"
                  variant="secondary"
                  onClick={() => void handleExportVideo()}
                  disabled={videoExporting}
                >
                  <Video className="h-4 w-4" />
                  {videoExporting ? t('export.videoExporting') : t('export.videoExportButton')}
                </Button>
              </>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Import Script Section */}
      <div className="pt-4 border-t">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileUp className="h-4 w-4" /> {t('project.importScript')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <Button
              variant="outline"
              onClick={() => {
                void handleImportSelect().catch(() => {});
              }}
            >
              <FolderOpen className="h-4 w-4" />
              {t('project.importSelectFile')}
            </Button>
          </CardContent>
        </Card>
      </div>

      {/* Import Mapping Dialog */}
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
