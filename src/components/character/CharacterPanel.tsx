import { useState } from 'react';
import { Plus, Pencil, Trash2, Save, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useCharacterStore } from '../../store/characterStore';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Slider } from '../ui/slider';
import { Card, CardContent } from '../ui/card';
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from '../ui/select';
import type { CharacterInput, Character } from '../../types';

const defaultInput: CharacterInput = {
    name: '',
    tts_model: 'qwen3-tts-flash',
    voice_name: 'Cherry',
    speed: 1.0,
    pitch: 1.0,
};

export default function CharacterPanel() {
    const { t } = useTranslation();
    const { characters, createCharacter, updateCharacter, deleteCharacter } = useCharacterStore();
    const [editing, setEditing] = useState<string | null>(null);
    const [form, setForm] = useState<CharacterInput>(defaultInput);
    const [isCreating, setIsCreating] = useState(false);

    const startCreate = () => {
        setIsCreating(true);
        setEditing(null);
        setForm(defaultInput);
    };

    const startEdit = (c: Character) => {
        setEditing(c.id);
        setIsCreating(false);
        setForm({
            name: c.name,
            tts_model: c.tts_model,
            voice_name: c.voice_name,
            speed: c.speed,
            pitch: c.pitch,
        });
    };

    const cancel = () => {
        setEditing(null);
        setIsCreating(false);
        setForm(defaultInput);
    };

    const handleSave = async () => {
        if (!form.name.trim()) return;
        if (isCreating) {
            await createCharacter(form);
        } else if (editing) {
            await updateCharacter(editing, form);
        }
        cancel();
    };

    const handleDelete = async (id: string) => {
        if (window.confirm(t('character.confirmDelete'))) {
            await deleteCharacter(id);
        }
    };

    const renderForm = () => (
        <Card>
            <CardContent className="space-y-4">
                <div className="space-y-1.5">
                    <Label>{t('character.name')}</Label>
                    <Input
                        value={form.name}
                        onChange={(e) => setForm({ ...form, name: e.target.value })}
                        placeholder={t('character.namePlaceholder')}
                        autoFocus
                    />
                </div>
                <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-1.5">
                        <Label>{t('character.ttsModel')}</Label>
                        <Select value={form.tts_model} onValueChange={(v) => setForm({ ...form, tts_model: v })}>
                            <SelectTrigger className="w-full">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="qwen3-tts-flash">Qwen3 TTS Flash</SelectItem>
                                <SelectItem value="qwen3-tts-instruct-flash">Qwen3 TTS Instruct Flash</SelectItem>
                                <SelectItem value="cosyvoice-v3-flash">CosyVoice v3 Flash</SelectItem>
                                <SelectItem value="cosyvoice-v3-plus">CosyVoice v3 Plus</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>
                    <div className="space-y-1.5">
                        <Label>{t('character.voice')}</Label>
                        <Input
                            value={form.voice_name}
                            onChange={(e) => setForm({ ...form, voice_name: e.target.value })}
                            placeholder="Cherry"
                        />
                    </div>
                </div>
                <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                        <Label>{t('character.speed')} ({form.speed.toFixed(1)}x)</Label>
                        <Slider
                            min={0.5} max={2.0} step={0.1}
                            value={[form.speed]}
                            onValueChange={([v]) => setForm({ ...form, speed: v })}
                        />
                    </div>
                    <div className="space-y-2">
                        <Label>{t('character.pitch')} ({form.pitch.toFixed(1)}x)</Label>
                        <Slider
                            min={0.5} max={2.0} step={0.1}
                            value={[form.pitch]}
                            onValueChange={([v]) => setForm({ ...form, pitch: v })}
                        />
                    </div>
                </div>
                <div className="flex gap-2 justify-end">
                    <Button variant="outline" onClick={cancel}>
                        <X className="h-4 w-4" /> {t('character.cancel')}
                    </Button>
                    <Button onClick={handleSave}>
                        <Save className="h-4 w-4" /> {t('character.save')}
                    </Button>
                </div>
            </CardContent>
        </Card>
    );

    return (
        <div className="mx-auto max-w-3xl px-6 py-8">
            <div className="flex items-center justify-between mb-6">
                <h2 className="text-xl font-bold">{t('character.title')}</h2>
                <Button onClick={startCreate}>
                    <Plus className="h-4 w-4" /> {t('character.create')}
                </Button>
            </div>
            {isCreating && renderForm()}
            <div className="space-y-3 mt-4">
                {characters.map((c) =>
                    editing === c.id ? (
                        <div key={c.id}>{renderForm()}</div>
                    ) : (
                        <Card key={c.id}>
                            <CardContent className="flex items-center justify-between">
                                <div>
                                    <p className="font-medium">{c.name}</p>
                                    <p className="text-sm text-muted-foreground">
                                        {c.tts_model} · {c.voice_name} · {t('character.speed')} {c.speed}x · {t('character.pitch')} {c.pitch}x
                                    </p>
                                </div>
                                <div className="flex gap-1">
                                    <Button variant="ghost" size="icon-sm" onClick={() => startEdit(c)} aria-label={`Edit ${c.name}`}>
                                        <Pencil className="h-4 w-4" />
                                    </Button>
                                    <Button variant="ghost" size="icon-sm" onClick={() => handleDelete(c.id)} aria-label={`Delete ${c.name}`} className="hover:text-destructive">
                                        <Trash2 className="h-4 w-4" />
                                    </Button>
                                </div>
                            </CardContent>
                        </Card>
                    ),
                )}
                {characters.length === 0 && !isCreating && (
                    <p className="text-center text-muted-foreground py-12">{t('character.empty')}</p>
                )}
            </div>
        </div>
    );
}
