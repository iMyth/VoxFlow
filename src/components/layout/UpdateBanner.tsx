import { useTranslation } from 'react-i18next';

interface UpdateBannerProps {
  updateAvailable: boolean;
  latestVersion: string;
  downloading: boolean;
  onInstall: () => void;
}

export default function UpdateBanner({ updateAvailable, latestVersion, downloading, onInstall }: UpdateBannerProps) {
  const { t } = useTranslation();

  if (!updateAvailable) return null;

  return (
    <div className="bg-gradient-to-r from-blue-600 to-indigo-600 text-white px-4 py-2 flex items-center justify-between text-sm shrink-0">
      <span>
        {t('update.available')} <strong>{t('update.version', { version: latestVersion })}</strong>
      </span>
      <button
        onClick={onInstall}
        disabled={downloading}
        className="ml-3 px-3 py-1 bg-white text-blue-700 rounded-md text-xs font-medium hover:bg-blue-50 disabled:opacity-50 transition-colors"
      >
        {downloading ? t('update.downloading') : t('update.installNow')}
      </button>
    </div>
  );
}
