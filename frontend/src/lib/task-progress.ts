export type TaskPhase =
  | 'idle'
  | 'selecting'
  | 'creating'
  | 'preparing'
  | 'converting'
  | 'uploading'
  | 'queued'
  | 'transcribing'
  | 'saving'
  | 'completed'
  | 'error'
  | 'cancelled';

export interface WeightedProgressConfig {
  [phase: string]: number;
}

export interface TaskProgressPayload {
  phase?: TaskPhase | string;
  message?: string;
  meeting_title?: string;
  folder_path?: string;
  meeting_id?: string;
  audio_path?: string;
  task_kind?: 'import' | 'retranscribe' | string;
  phase_progress?: number;
  overall_progress?: number;
  job_id?: string;
}

export const IMPORT_PROGRESS_WEIGHTS: WeightedProgressConfig = {
  creating: 5,
  converting: 10,
  uploading: 30,
  queued: 5,
  transcribing: 40,
  saving: 10,
};

export const RETRANSCRIBE_PROGRESS_WEIGHTS: WeightedProgressConfig = {
  preparing: 10,
  converting: 10,
  uploading: 30,
  queued: 5,
  transcribing: 35,
  saving: 10,
};

const clamp = (value: number, min = 0, max = 100) => Math.min(max, Math.max(min, value));

export function computeWeightedProgress(
  phase: string | undefined,
  phaseProgress: number | undefined,
  weights: WeightedProgressConfig
): number {
  if (!phase) return 0;
  if (phase === 'completed') return 100;
  if (phase === 'error' || phase === 'cancelled') return 0;

  const entries = Object.entries(weights);
  const phaseIndex = entries.findIndex(([key]) => key === phase);
  if (phaseIndex === -1) return clamp(phaseProgress ?? 0);

  const completedWeight = entries
    .slice(0, phaseIndex)
    .reduce((sum, [, weight]) => sum + weight, 0);
  const currentWeight = entries[phaseIndex]?.[1] ?? 0;
  const normalizedPhaseProgress = clamp(phaseProgress ?? 0) / 100;

  return clamp(completedWeight + currentWeight * normalizedPhaseProgress);
}

export function getProgressLabel(phase: string | undefined): string {
  switch (phase) {
    case 'creating':
      return 'Creating meeting';
    case 'preparing':
      return 'Preparing task';
    case 'converting':
      return 'Preparing audio';
    case 'uploading':
      return 'Uploading';
    case 'queued':
      return 'Remote job queued';
    case 'transcribing':
      return 'Transcribing';
    case 'saving':
      return 'Saving result';
    case 'completed':
      return 'Completed';
    case 'error':
      return 'Failed';
    default:
      return 'Processing';
  }
}