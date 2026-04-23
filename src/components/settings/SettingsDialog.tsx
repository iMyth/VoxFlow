import { Save, KeyRound } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { AVAILABLE_VOICES } from '../../lib/voices';
import { useSettingsStore } from '../../store/settingsStore';
import { Button } from '../ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '../ui/dialog';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Slider } from '../ui/slider';

interface SettingsDialogProps {
  onClose: () => void;
}

export default function SettingsDialog({ onClose }: SettingsDialogProps) {
  const { t, i18n } = useTranslation();
  const settings = useSettingsStore();
  const [apiKey, setApiKey] = useState('');
  const [localEndpoint, setLocalEndpoint] = useState(settings.llmEndpoint);
  const [localModel, setLocalModel] = useState(settings.llmModel);
  const [localTtsModel, setLocalTtsModel] = useState(settings.defaultTtsModel);
  const [localVoice, setLocalVoice] = useState(settings.defaultVoiceName);
  const [localSpeed, setLocalSpeed] = useState(settings.defaultSpeed);
  const [localPitch, setLocalPitch] = useState(settings.defaultPitch);

  useEffect(() => {
    void settings.loadSettings();
    void settings.loadApiKey('dashscope').then((k) => {
      if (k) setApiKey(k);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    setLocalEndpoint(settings.llmEndpoint);
    setLocalModel(settings.llmModel);
    setLocalTtsModel(settings.defaultTtsModel);
    setLocalVoice(settings.defaultVoiceName);
    setLocalSpeed(settings.defaultSpeed);
    setLocalPitch(settings.defaultPitch);
  }, [
    settings.llmEndpoint,
    settings.llmModel,
    settings.defaultTtsModel,
    settings.defaultVoiceName,
    settings.defaultSpeed,
    settings.defaultPitch,
  ]);

  const handleLanguageChange = (lang: string) => {
    void i18n.changeLanguage(lang);
    localStorage.setItem('app-language', lang);
  };

  const handleSave = async () => {
    settings.set({
      llmEndpoint: localEndpoint,
      llmModel: localModel,
      defaultTtsModel: localTtsModel,
      defaultVoiceName: localVoice,
      defaultSpeed: localSpeed,
      defaultPitch: localPitch,
    });
    await settings.saveSettings();
    if (apiKey) await settings.saveApiKey('dashscope', apiKey);
    onClose();
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="sm:max-w-lg max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t('settings.title')}</DialogTitle>
        </DialogHeader>

        {/* Language */}
        <section className="space-y-3">
          <h3 className="text-sm font-semibold">{t('settings.language')}</h3>
          <Select value={i18n.language} onValueChange={handleLanguageChange}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="zh">{t('editor.languageZh')}</SelectItem>
              <SelectItem value="en">{t('editor.languageEn')}</SelectItem>
            </SelectContent>
          </Select>
        </section>

        {/* API Key */}
        <section className="space-y-3">
          <h3 className="text-sm font-semibold flex items-center gap-2">
            <KeyRound className="h-4 w-4" /> {t('settings.apiKeySection')}
          </h3>
          <div className="space-y-1.5">
            <Label>DashScope API Key</Label>
            <Input
              type="password"
              value={apiKey}
              onChange={(e) => {
                setApiKey(e.target.value);
              }}
              placeholder="sk-..."
            />
          </div>
          {!apiKey && <p className="text-xs text-amber-600 dark:text-amber-400">{t('settings.apiKeyHint')}</p>}
        </section>

        {/* LLM */}
        <section className="space-y-3">
          <h3 className="text-sm font-semibold">{t('settings.llmSection')}</h3>
          <div className="space-y-1.5">
            <Label>{t('settings.apiEndpoint')}</Label>
            <Input
              value={localEndpoint}
              onChange={(e) => {
                setLocalEndpoint(e.target.value);
              }}
              placeholder="https://dashscope.aliyuncs.com/compatible-mode/v1"
            />
          </div>
          <div className="space-y-1.5">
            <Label>{t('settings.modelName')}</Label>
            <Input
              value={localModel}
              onChange={(e) => {
                setLocalModel(e.target.value);
              }}
              placeholder="qwen3.6-plus"
            />
          </div>
        </section>

        {/* TTS */}
        <section className="space-y-3">
          <h3 className="text-sm font-semibold">{t('settings.ttsSection')}</h3>
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>{t('settings.ttsModel')}</Label>
              <Select value={localTtsModel} onValueChange={setLocalTtsModel}>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="qwen3-tts-flash">Qwen3 TTS Flash</SelectItem>
                  <SelectItem value="qwen3-tts-instruct-flash">Qwen3 TTS Instruct Flash</SelectItem>
                  <SelectItem value="qwen3-tts-instruct-flash-realtime">Qwen3 TTS Instruct Flash Realtime</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>{t('settings.defaultVoice')}</Label>
              <Select value={localVoice} onValueChange={setLocalVoice}>
                <SelectTrigger className="w-full">
                  <SelectValue placeholder={t('settings.selectVoice')} />
                </SelectTrigger>
                <SelectContent className="max-h-60">
                  {AVAILABLE_VOICES.map((v) => (
                    <SelectItem key={v.id} value={v.name}>
                      {v.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {(() => {
                const matched = AVAILABLE_VOICES.find((v) => v.name === localVoice);
                return matched?.description ? (
                  <p className="text-xs text-muted-foreground">{matched.description}</p>
                ) : null;
              })()}
            </div>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label>
                {t('settings.defaultSpeed')} ({localSpeed.toFixed(1)}x)
              </Label>
              <Slider
                min={0.5}
                max={2.0}
                step={0.1}
                value={[localSpeed]}
                onValueChange={([v]) => {
                  setLocalSpeed(v);
                }}
              />
            </div>
            <div className="space-y-2">
              <Label>
                {t('settings.defaultPitch')} ({localPitch.toFixed(1)}x)
              </Label>
              <Slider
                min={0.5}
                max={2.0}
                step={0.1}
                value={[localPitch]}
                onValueChange={([v]) => {
                  setLocalPitch(v);
                }}
              />
            </div>
          </div>
        </section>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('settings.cancel')}
          </Button>
          <Button onClick={() => void handleSave()}>
            <Save className="h-4 w-4" /> {t('settings.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
