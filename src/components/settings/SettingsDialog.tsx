import { useEffect, useState } from 'react';
import { Save, KeyRound } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '../../store/settingsStore';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Slider } from '../ui/slider';
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from '../ui/select';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogFooter,
} from '../ui/dialog';

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
        settings.loadSettings();
        settings.loadApiKey('dashscope').then((k) => k && setApiKey(k));
    }, []);

    useEffect(() => {
        setLocalEndpoint(settings.llmEndpoint);
        setLocalModel(settings.llmModel);
        setLocalTtsModel(settings.defaultTtsModel);
        setLocalVoice(settings.defaultVoiceName);
        setLocalSpeed(settings.defaultSpeed);
        setLocalPitch(settings.defaultPitch);
    }, [settings.llmEndpoint, settings.llmModel, settings.defaultTtsModel, settings.defaultVoiceName, settings.defaultSpeed, settings.defaultPitch]);

    const handleLanguageChange = (lang: string) => {
        i18n.changeLanguage(lang);
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
        <Dialog open onOpenChange={(open) => !open && onClose()}>
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
                            <SelectItem value="zh">中文</SelectItem>
                            <SelectItem value="en">English</SelectItem>
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
                            onChange={(e) => setApiKey(e.target.value)}
                            placeholder="sk-..."
                        />
                    </div>
                    {!apiKey && (
                        <p className="text-xs text-amber-600 dark:text-amber-400">
                            {t('settings.apiKeyHint')}
                        </p>
                    )}
                </section>

                {/* LLM */}
                <section className="space-y-3">
                    <h3 className="text-sm font-semibold">{t('settings.llmSection')}</h3>
                    <div className="space-y-1.5">
                        <Label>{t('settings.apiEndpoint')}</Label>
                        <Input
                            value={localEndpoint}
                            onChange={(e) => setLocalEndpoint(e.target.value)}
                            placeholder="https://dashscope.aliyuncs.com/compatible-mode/v1"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label>{t('settings.modelName')}</Label>
                        <Input
                            value={localModel}
                            onChange={(e) => setLocalModel(e.target.value)}
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
                                    <SelectItem value="qwen3-tts-instruct-flash-realtime">Qwen3 TTS Instruct Flash</SelectItem>
                                </SelectContent>
                            </Select>
                        </div>
                        <div className="space-y-1.5">
                            <Label>{t('settings.defaultVoice')}</Label>
                            <Input
                                value={localVoice}
                                onChange={(e) => setLocalVoice(e.target.value)}
                                placeholder="Cherry"
                            />
                        </div>
                    </div>
                    <div className="grid grid-cols-2 gap-4">
                        <div className="space-y-2">
                            <Label>{t('settings.defaultSpeed')} ({localSpeed.toFixed(1)}x)</Label>
                            <Slider
                                min={0.5} max={2.0} step={0.1}
                                value={[localSpeed]}
                                onValueChange={([v]) => setLocalSpeed(v)}
                            />
                        </div>
                        <div className="space-y-2">
                            <Label>{t('settings.defaultPitch')} ({localPitch.toFixed(1)}x)</Label>
                            <Slider
                                min={0.5} max={2.0} step={0.1}
                                value={[localPitch]}
                                onValueChange={([v]) => setLocalPitch(v)}
                            />
                        </div>
                    </div>
                </section>

                <DialogFooter>
                    <Button variant="outline" onClick={onClose}>{t('settings.cancel')}</Button>
                    <Button onClick={handleSave}>
                        <Save className="h-4 w-4" /> {t('settings.save')}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
