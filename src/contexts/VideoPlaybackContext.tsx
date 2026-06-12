import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';

interface VideoPlaybackContextValue {
  /** 当前正在播放的 section_id，null 表示无播放 */
  playingSectionId: string | null;
  /** 请求播放某个 section 的视频，会暂停其他正在播放的视频 */
  requestPlay: (sectionId: string) => void;
  /** 通知某个 section 的视频已暂停/停止 */
  notifyPause: (sectionId: string) => void;
  /** 暂停所有正在播放的视频 */
  pauseAll: () => void;
}

export const VideoPlaybackContext = createContext<VideoPlaybackContextValue>({
  playingSectionId: null,
  requestPlay: () => {},
  notifyPause: () => {},
  pauseAll: () => {},
});

export function VideoPlaybackProvider({ children }: { children: ReactNode }) {
  const [playingSectionId, setPlayingSectionId] = useState<string | null>(null);

  const requestPlay = useCallback((sectionId: string) => {
    setPlayingSectionId(sectionId);
  }, []);

  const notifyPause = useCallback((sectionId: string) => {
    setPlayingSectionId((current) => (current === sectionId ? null : current));
  }, []);

  const pauseAll = useCallback(() => {
    setPlayingSectionId(null);
  }, []);

  // Cleanup on unmount: pause all playing videos when navigating away
  useEffect(() => {
    return () => {
      // When the provider unmounts (e.g., tab switch), set playingSectionId to null.
      // This ensures any playing video is signaled to pause before DOM removal.
      // React unmount of child components will remove <video> elements from DOM.
      setPlayingSectionId(null);
    };
  }, []);

  return (
    <VideoPlaybackContext.Provider value={{ playingSectionId, requestPlay, notifyPause, pauseAll }}>
      {children}
    </VideoPlaybackContext.Provider>
  );
}

export function useVideoPlayback(): VideoPlaybackContextValue {
  return useContext(VideoPlaybackContext);
}
