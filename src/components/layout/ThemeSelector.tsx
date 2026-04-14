import { Moon, Sun, Laptop } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useThemeStore, type ThemeMode } from '../../store/themeStore';
import { Button } from '../ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/tooltip';

export default function ThemeSelector() {
  const { t } = useTranslation();
  const { theme, setTheme } = useThemeStore();
  const [menuOpen, setMenuOpen] = useState(false);

  const themeIcon =
    theme === 'dark' ? (
      <Moon className="h-5 w-5" />
    ) : theme === 'light' ? (
      <Sun className="h-5 w-5" />
    ) : (
      <Laptop className="h-5 w-5" />
    );

  const options: { value: ThemeMode; icon: typeof Sun; label: string }[] = [
    { value: 'system', icon: Laptop, label: t('app.themeSystem') },
    { value: 'light', icon: Sun, label: t('app.themeLight') },
    { value: 'dark', icon: Moon, label: t('app.themeDark') },
  ];

  return (
    <div className="relative">
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              onClick={() => {
                setMenuOpen(!menuOpen);
              }}
            >
              {themeIcon}
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t('app.theme')}</TooltipContent>
        </Tooltip>
      </TooltipProvider>
      {menuOpen && (
        <>
          <div
            className="fixed inset-0 z-40"
            onClick={() => {
              setMenuOpen(false);
            }}
          />
          <div className="absolute right-0 top-full mt-1 z-50 w-40 rounded-lg border bg-popover p-1 shadow-lg">
            {options.map(({ value, icon: Icon, label }) => (
              <button
                key={value}
                className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent ${theme === value ? 'bg-accent font-medium' : ''}`}
                onClick={() => {
                  setTheme(value);
                  setMenuOpen(false);
                }}
              >
                <Icon className="h-4 w-4" />
                {label}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
