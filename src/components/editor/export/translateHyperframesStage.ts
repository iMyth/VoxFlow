/** Translate backend stage keys to localized progress messages. */
export function translateHyperframesStage(
  stage: string,
  t: (key: string, opts?: Record<string, unknown>) => string
): string {
  if (!stage) return t('export.hyperframesGenerating');

  // Handle parameterized keys like "retrying:1/2", "generating_chunk:2/5/第一章"
  const [key, ...params] = stage.split(':');

  switch (key) {
    // Agent mode
    case 'loading_skills':
      return t('export.hfStageLoadingSkills');
    case 'building_agent':
      return t('export.hfStageBuildingAgent');
    case 'agent_generating':
      return t('export.hfStageAgentGenerating');
    case 'extracting_html':
      return t('export.hfStageExtractingHtml');
    case 'agent_done':
      return t('export.hfStageAgentDone');
    case 'agent_done_with_warnings':
      return t('export.hfStageAgentDoneWarnings');

    // Single-shot mode
    case 'generating':
      return t('export.hfStageGenerating');
    case 'validating':
      return t('export.hfStageValidating');
    case 'retrying': {
      const [current, total] = (params[0] ?? '').split('/');
      return t('export.hfStageRetrying', { current, total });
    }

    // Orchestrated mode
    case 'planning':
      return t('export.hfStagePlanning');
    case 'planned': {
      const chunks = params[0] ?? '';
      return t('export.hfStagePlanned', { chunks });
    }
    case 'workers_start': {
      const [total] = (params[0] ?? '').split('/');
      return t('export.hfStageWorkersStart', { total });
    }
    case 'chunk_done': {
      return t('export.hfStageChunkDone', { index: params[0] });
    }
    case 'chunk_failed': {
      return t('export.hfStageChunkFailed', { index: params[0] });
    }
    case 'workers_done': {
      const [total, success, failed] = (params[0] ?? '').split('/');
      return t('export.hfStageWorkersDone', { total, success, failed });
    }
    case 'partial_failure': {
      const [failed, total] = (params[0] ?? '').split('/');
      return t('export.hfStagePartialFailure', { failed, total });
    }
    case 'skipped_entry':
    case 'skipped_entries':
      return t('export.hfStageSkipped', { count: params[0] });
    case 'merging':
      return t('export.hfStageMerging');
    case 'orchestrated_done':
      return t('export.hfStageDone');

    // Chunked mode
    case 'chunked_start': {
      return t('export.hfStageChunkedStart', { total: params[0] });
    }
    case 'generating_chunk': {
      const [current, total, section] = (params[0] ?? '').split('/');
      return t('export.hfStageGeneratingChunk', { current, total, section });
    }
    case 'chunk_retry':
      return t('export.hfStageChunkRetry', { index: params[0] });

    default:
      // Fallback: show the raw stage (for any unrecognized keys)
      return stage;
  }
}
