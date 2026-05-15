import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { save, open } from '@tauri-apps/plugin-dialog';
import {
  Download,
  AlertTriangle,
  CheckCircle,
  Loader2,
  FolderOpen,
  Play,
  Pause,
  FileUp,
  Video,
  Image,
  Lock,
  ArrowRight,
  Sparkles,
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
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Progress } from '../ui/progress';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Slider } from '../ui/slider';
import { Tabs, TabsList, TabsTrigger } from '../ui/tabs';

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
  const [videoStyle, setVideoStyle] = useState<VideoStyle | 'hyperframes'>('particles');
  const [videoFgColor, setVideoFgColor] = useState('6366f1');
  const [videoBgColor, setVideoBgColor] = useState('0a0a1a');
  const [videoBgImage, setVideoBgImage] = useState<string | null>(null);
  const [videoExporting, setVideoExporting] = useState(false);
  const [videoProgress, setVideoProgress] = useState<MixProgress | null>(null);
  const [videoDone, setVideoDone] = useState(false);
  const [videoError, setVideoError] = useState<string | null>(null);
  const [lastExportedAudioPath, setLastExportedAudioPath] = useState<string | null>(null);

  // Hyperframes export state
  type HyperframesTemplate = 'minimal-subtitle' | 'dialogue-cards' | 'chapter-sections';
  type HyperframesMode = 'template' | 'ai';
  const [hyperframesMode, setHyperframesMode] = useState<HyperframesMode>('template');
  const [hyperframesTemplate, setHyperframesTemplate] = useState<HyperframesTemplate>('minimal-subtitle');
  const [hyperframesExporting, setHyperframesExporting] = useState(false);
  const [hyperframesProgress, setHyperframesProgress] = useState<{ percent: number; stage: string } | null>(null);
  const [hyperframesDone, setHyperframesDone] = useState(false);
  const [hyperframesError, setHyperframesError] = useState<string | null>(null);
  const [hyperframesOutputDir, setHyperframesOutputDir] = useState<string | null>(null);

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
        style: videoStyle as VideoStyle,
        width: 1280,
        height: 720,
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

  const handleCancelVideo = async () => {
    await ipc.cancelVideoExport();
    setVideoExporting(false);
    setVideoProgress(null);
  };

  const handleExportHyperframes = async () => {
    if (!currentProject) return;

    const selectedDir = await open({
      title: '选择 Hyperframes 项目输出目录',
      directory: true,
      multiple: false,
    });
    if (!selectedDir) return;

    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    const outputDir: string = Array.isArray(selectedDir) ? selectedDir[0] : selectedDir;

    setHyperframesExporting(true);
    setHyperframesProgress(null);
    setHyperframesDone(false);
    setHyperframesError(null);

    const unlisten = await ipc.onHyperframesProgress((p: { percent: number; stage: string }) => {
      setHyperframesProgress(p);
    });

    try {
      await ipc.exportHyperframes({
        project_id: currentProject.project.id,
        output_dir: outputDir,
        template: hyperframesTemplate,
        include_audio: !!lastExportedAudioPath,
        audio_path: lastExportedAudioPath,
        use_ai: hyperframesMode === 'ai',
      });
      setHyperframesDone(true);
      setHyperframesOutputDir(outputDir);
    } catch (e) {
      setHyperframesError(String(e));
    } finally {
      unlisten();
      setHyperframesExporting(false);
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

  const audioReady = done && !!lastExportedAudioPath;

  return (
    <div className="mx-auto max-w-3xl px-6 py-8">
      {/* Page header */}
      <div className="mb-8">
        <h2 className="text-xl font-bold tracking-tight">{t('export.title')}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t('export.videoNeedAudioFirst')}</p>
      </div>

      {/* Missing audio warning */}
      {missingLines.length > 0 && (
        <Alert variant="destructive" className="mb-6">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('export.missingAudio', { count: missingLines.length })}</AlertTitle>
          <AlertDescription>
            <ul className="mt-1 space-y-0.5 text-xs">
              {missingLines.slice(0, 5).map((l) => (
                <li key={l.id}>{t('export.missingLine', { line: l.line_order + 1, text: l.text.slice(0, 40) })}</li>
              ))}
              {missingLines.length > 5 && <li>{t('export.missingMore', { count: missingLines.length - 5 })}</li>}
            </ul>
            <p className="mt-2 text-xs">{t('export.missingHint')}</p>
          </AlertDescription>
        </Alert>
      )}

      {/* Pipeline Steps */}
      <div className="space-y-4">
        {/* ===== STEP 1: Audio Export ===== */}
        <div className="relative rounded-xl border border-border bg-card overflow-hidden">
          {/* Step header */}
          <div className="flex items-center gap-3 px-5 py-3 border-b border-border/50 bg-muted/30">
            <div
              className={`flex h-7 w-7 items-center justify-center rounded-full text-xs font-bold ${audioReady ? 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400' : 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-400'}`}
            >
              {audioReady ? <CheckCircle className="h-4 w-4" /> : '1'}
            </div>
            <div className="flex-1">
              <h3 className="text-sm font-semibold">{t('export.exportButton')}</h3>
            </div>
            {audioReady && (
              <span className="text-xs text-green-600 dark:text-green-400 font-medium">
                {t('export.exportSuccess')}
              </span>
            )}
          </div>

          {/* Step content */}
          <div className="px-5 py-4 space-y-4">
            {/* BGM section */}
            <div className="space-y-3">
              <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                {t('export.bgm')}
              </Label>
              <div className="flex gap-2 items-center">
                <Input
                  className="flex-1 h-9"
                  placeholder={t('export.bgmPlaceholder')}
                  value={bgmPath ?? ''}
                  onChange={(e) => {
                    setBgmPath(e.target.value || null);
                  }}
                />
                <Button
                  variant="outline"
                  size="icon"
                  className="h-9 w-9 shrink-0"
                  onClick={() => void handleBgmBrowse()}
                  title={t('export.browse')}
                >
                  <FolderOpen className="h-4 w-4" />
                </Button>
                {bgmPath && (
                  <Button
                    variant="outline"
                    size="icon"
                    className="h-9 w-9 shrink-0"
                    onClick={() => void toggleBgmPreview()}
                    title={bgmPlaying ? t('editor.pause') : t('editor.play')}
                  >
                    {bgmPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                  </Button>
                )}
              </div>
              {bgmPath && (
                <div className="space-y-1.5">
                  <Label className="text-xs">{t('export.bgmVolume', { percent: Math.round(bgmVolume * 100) })}</Label>
                  <Slider
                    min={0}
                    max={1}
                    step={0.05}
                    value={[bgmVolume]}
                    onValueChange={(v) => void handleBgmVolumeChange(v)}
                  />
                </div>
              )}
            </div>

            {/* Output filename */}
            <div className="space-y-1.5">
              <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                {t('export.outputLabel')}
              </Label>
              <Input
                className="h-9"
                value={outputPath}
                onChange={(e) => {
                  setOutputPath(e.target.value);
                }}
              />
            </div>

            {/* Progress */}
            {exporting && progress && (
              <div className="rounded-lg bg-muted/50 p-3 space-y-2">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>{progress.stage}</span>
                  <span className="ml-auto font-mono">{Math.round(progress.percent)}%</span>
                </div>
                <Progress value={progress.percent} className="h-1.5" />
              </div>
            )}

            {error && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>{t('export.exportFailed')}</AlertTitle>
                <AlertDescription className="text-xs">{error}</AlertDescription>
              </Alert>
            )}

            {/* Export button */}
            <Button
              className="w-full gap-2"
              onClick={() => void handleExport()}
              disabled={exporting || missingLines.length > 0 || !outputPath.trim()}
            >
              {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
              {exporting ? t('export.exporting') : t('export.exportButton')}
            </Button>
          </div>
        </div>

        {/* Arrow connector */}
        <div className="flex justify-center">
          <div
            className={`flex items-center gap-1.5 text-xs ${audioReady ? 'text-green-600 dark:text-green-400' : 'text-muted-foreground/40'}`}
          >
            <ArrowRight className="h-4 w-4" />
          </div>
        </div>

        {/* ===== STEP 2: Video Export ===== */}
        <div
          className={`relative rounded-xl border overflow-hidden transition-all duration-300 ${audioReady || videoStyle === 'hyperframes' ? 'border-border bg-card' : 'border-border/40 bg-muted/20'}`}
        >
          {/* Locked overlay — only for non-hyperframes video styles */}
          {!audioReady && videoStyle !== 'hyperframes' && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/60 backdrop-blur-[1px]">
              <div className="flex flex-col items-center gap-2 text-center px-4">
                <div className="flex h-10 w-10 items-center justify-center rounded-full bg-muted">
                  <Lock className="h-4 w-4 text-muted-foreground" />
                </div>
                <p className="text-xs text-muted-foreground max-w-[200px]">{t('export.videoNeedAudioFirst')}</p>
              </div>
            </div>
          )}

          {/* Step header */}
          <div className="flex items-center gap-3 px-5 py-3 border-b border-border/50 bg-muted/30">
            <div
              className={`flex h-7 w-7 items-center justify-center rounded-full text-xs font-bold ${videoDone || hyperframesDone ? 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400' : audioReady || videoStyle === 'hyperframes' ? 'bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-400' : 'bg-muted text-muted-foreground'}`}
            >
              {videoDone || hyperframesDone ? <CheckCircle className="h-4 w-4" /> : '2'}
            </div>
            <div className="flex-1">
              <h3 className="text-sm font-semibold">{t('export.videoTitle')}</h3>
            </div>
            {(videoDone || hyperframesDone) && (
              <span className="text-xs text-green-600 dark:text-green-400 font-medium">
                {videoStyle === 'hyperframes' ? 'Hyperframes 导出成功' : t('export.videoExportSuccess')}
              </span>
            )}
          </div>

          {/* Step content */}
          <div className="px-5 py-4 space-y-4">
            {/* Style selector */}
            <div className="space-y-1.5">
              <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                {t('export.videoStyle')}
              </Label>
              <Select
                value={videoStyle}
                onValueChange={(v) => {
                  const style = v as VideoStyle | 'hyperframes';
                  setVideoStyle(style);
                  // Set recommended default colors per style
                  switch (style) {
                    case 'fractal':
                      setVideoFgColor('e8a838');
                      setVideoBgColor('050510');
                      break;
                    case 'starfield':
                      setVideoFgColor('7c9ff5');
                      setVideoBgColor('020208');
                      break;
                    case 'vinyl':
                      setVideoFgColor('6366f1');
                      setVideoBgColor('1a1a2e');
                      break;
                    case 'particles':
                      setVideoFgColor('6366f1');
                      setVideoBgColor('0a0a1a');
                      break;
                  }
                }}
              >
                <SelectTrigger className="h-9">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="particles">{t('export.styleParticles')}</SelectItem>
                  <SelectItem value="starfield">{t('export.styleStarfield')}</SelectItem>
                  <SelectItem value="vinyl">{t('export.styleVinyl')}</SelectItem>
                  <SelectItem value="fractal">{t('export.styleFractal')}</SelectItem>
                  <SelectItem value="hyperframes">
                    <span className="flex items-center gap-1.5">
                      <Sparkles className="h-3.5 w-3.5" />
                      Hyperframes
                    </span>
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Colors — only for non-hyperframes styles */}
            {videoStyle !== 'hyperframes' && (
              <>
                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t('export.videoFgColor')}</Label>
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground">#</span>
                      <Input
                        className="flex-1 h-9"
                        value={videoFgColor}
                        onChange={(e) => {
                          setVideoFgColor(e.target.value.replace(/[^0-9a-fA-F]/g, '').slice(0, 6));
                        }}
                        maxLength={6}
                      />
                      <div
                        className="w-9 h-9 rounded-md border shrink-0"
                        style={{ backgroundColor: `#${videoFgColor}` }}
                      />
                    </div>
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t('export.videoBgColor')}</Label>
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground">#</span>
                      <Input
                        className="flex-1 h-9"
                        value={videoBgColor}
                        onChange={(e) => {
                          setVideoBgColor(e.target.value.replace(/[^0-9a-fA-F]/g, '').slice(0, 6));
                        }}
                        maxLength={6}
                      />
                      <div
                        className="w-9 h-9 rounded-md border shrink-0"
                        style={{ backgroundColor: `#${videoBgColor}` }}
                      />
                    </div>
                  </div>
                </div>

                {/* Background image */}
                <div className="space-y-1.5">
                  <Label className="text-xs">{t('export.videoBgImage')}</Label>
                  <div className="flex gap-2 items-center">
                    <Input
                      className="flex-1 h-9"
                      placeholder={t('export.videoBgImagePlaceholder')}
                      value={videoBgImage ?? ''}
                      readOnly
                    />
                    <Button
                      variant="outline"
                      size="icon"
                      className="h-9 w-9 shrink-0"
                      onClick={() => void handleVideoBgImageBrowse()}
                    >
                      <Image className="h-4 w-4" />
                    </Button>
                    {videoBgImage && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-9 w-9 shrink-0"
                        onClick={() => {
                          setVideoBgImage(null);
                        }}
                      >
                        ✕
                      </Button>
                    )}
                  </div>
                  <p className="text-[11px] text-muted-foreground">{t('export.videoBgImageHint')}</p>
                </div>
              </>
            )}

            {/* Hyperframes options — mode toggle + template picker */}
            {videoStyle === 'hyperframes' && (
              <div className="space-y-4">
                {/* Mode toggle: 固定模板 / AI 生成 */}
                <div className="space-y-1.5">
                  <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">生成模式</Label>
                  <Tabs
                    value={hyperframesMode}
                    onValueChange={(v) => {
                      setHyperframesMode(v as HyperframesMode);
                    }}
                  >
                    <TabsList className="w-full">
                      <TabsTrigger value="template" className="flex-1">
                        固定模板
                      </TabsTrigger>
                      <TabsTrigger value="ai" className="flex-1">
                        <Sparkles className="h-3.5 w-3.5 mr-1" />
                        AI 生成
                      </TabsTrigger>
                    </TabsList>
                  </Tabs>
                </div>

                {/* Template picker — only shown in template mode */}
                {hyperframesMode === 'template' && (
                  <div className="space-y-1.5">
                    <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                      选择模板
                    </Label>
                    <div className="grid gap-2">
                      {[
                        { id: 'minimal-subtitle' as const, name: '简约字幕', desc: '深色背景 + 居中白色文字' },
                        { id: 'dialogue-cards' as const, name: '对话卡片', desc: '角色对话气泡' },
                        { id: 'chapter-sections' as const, name: '章节分段', desc: '按章节分段 + 标题卡片' },
                      ].map((tmpl) => (
                        <button
                          key={tmpl.id}
                          type="button"
                          className={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors ${
                            hyperframesTemplate === tmpl.id
                              ? 'border-primary bg-primary/5'
                              : 'border-border hover:border-primary/50 hover:bg-muted/50'
                          }`}
                          onClick={() => {
                            setHyperframesTemplate(tmpl.id);
                          }}
                        >
                          <div
                            className={`mt-0.5 h-3 w-3 rounded-full border-2 shrink-0 ${
                              hyperframesTemplate === tmpl.id
                                ? 'border-primary bg-primary'
                                : 'border-muted-foreground/40'
                            }`}
                          />
                          <div>
                            <div className="text-sm font-medium">{tmpl.name}</div>
                            <div className="text-xs text-muted-foreground">{tmpl.desc}</div>
                          </div>
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                {/* AI mode hint */}
                {hyperframesMode === 'ai' && (
                  <div className="rounded-lg bg-muted/50 p-3">
                    <p className="text-xs text-muted-foreground">
                      AI 将根据文案内容自由创作视觉画面，使用 CSS 动画、SVG、GSAP 等技术表达文案意境。
                    </p>
                  </div>
                )}
              </div>
            )}

            {/* Video progress */}
            {videoExporting && videoProgress && videoStyle !== 'hyperframes' && (
              <div className="rounded-lg bg-muted/50 p-3 space-y-2">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>{videoProgress.stage}</span>
                  <span className="ml-auto font-mono">{Math.round(videoProgress.percent)}%</span>
                </div>
                <Progress value={videoProgress.percent} className="h-1.5" />
              </div>
            )}

            {/* Hyperframes progress */}
            {hyperframesExporting && hyperframesProgress && (
              <div className="rounded-lg bg-muted/50 p-3 space-y-2">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>{hyperframesProgress.stage}</span>
                  <span className="ml-auto font-mono">{Math.round(hyperframesProgress.percent)}%</span>
                </div>
                <Progress value={hyperframesProgress.percent} className="h-1.5" />
              </div>
            )}

            {videoStyle !== 'hyperframes' && videoError && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>{t('export.videoExportFailed')}</AlertTitle>
                <AlertDescription className="text-xs">{videoError}</AlertDescription>
              </Alert>
            )}

            {videoStyle === 'hyperframes' && hyperframesError && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>Hyperframes 导出失败</AlertTitle>
                <AlertDescription className="text-xs">{hyperframesError}</AlertDescription>
              </Alert>
            )}

            {/* Hyperframes success */}
            {videoStyle === 'hyperframes' && hyperframesDone && hyperframesOutputDir && (
              <Alert>
                <CheckCircle className="h-4 w-4 text-green-500" />
                <AlertTitle>Hyperframes 项目导出成功</AlertTitle>
                <AlertDescription className="text-xs space-y-1 mt-1">
                  <p className="font-medium">后续操作：</p>
                  <code className="block bg-muted rounded px-2 py-1 text-[11px]">
                    cd {hyperframesOutputDir} && npx hyperframes preview
                  </code>
                  <code className="block bg-muted rounded px-2 py-1 text-[11px]">
                    npx hyperframes render --output output.mp4
                  </code>
                  {lastExportedAudioPath && (
                    <code className="block bg-muted rounded px-2 py-1 text-[11px]">
                      ffmpeg -i output.mp4 -i assets/audio.mp3 -c:v copy -c:a aac final.mp4
                    </code>
                  )}
                </AlertDescription>
              </Alert>
            )}

            {/* Video/Hyperframes export button */}
            <div className="flex gap-2">
              {videoStyle === 'hyperframes' ? (
                <Button
                  className="flex-1 gap-2"
                  variant="secondary"
                  onClick={() => void handleExportHyperframes()}
                  disabled={hyperframesExporting || !currentProject}
                >
                  {hyperframesExporting ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Sparkles className="h-4 w-4" />
                  )}
                  {hyperframesExporting ? '正在导出...' : '导出 Hyperframes 项目'}
                </Button>
              ) : (
                <Button
                  className="flex-1 gap-2"
                  variant="secondary"
                  onClick={() => void handleExportVideo()}
                  disabled={videoExporting || !audioReady}
                >
                  {videoExporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Video className="h-4 w-4" />}
                  {videoExporting ? t('export.videoExporting') : t('export.videoExportButton')}
                </Button>
              )}
              {videoExporting && videoStyle !== 'hyperframes' && (
                <Button
                  variant="outline"
                  className="gap-1.5 text-destructive border-destructive/30 hover:bg-destructive/10"
                  onClick={() => void handleCancelVideo()}
                >
                  {t('export.cancelVideo')}
                </Button>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Import section — secondary, below the pipeline */}
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
            onClick={() => {
              void handleImportSelect().catch(() => {});
            }}
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
