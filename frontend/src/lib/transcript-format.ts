import type { Transcript } from '../types';

export interface TranscriptSegment {
  text: string;
  speaker: string | null;
  start?: number;
  end?: number;
  words?: {word: string; start: number; end: number; probability?: number}[];
}

/** Normalize legacy JSON strings, wrapped exports and native transcript rows. */
export function normalizeTranscript(input: unknown, depth = 0): TranscriptSegment[] {
  if (depth > 5) throw new Error('Format de transcription invalide.');
  if (typeof input === 'string') {
    try { return normalizeTranscript(JSON.parse(input), depth + 1); }
    catch { throw new Error('Format de transcription invalide.'); }
  }
  if (input && typeof input === 'object' && !Array.isArray(input)) {
    const wrapper = input as Record<string, unknown>;
    return normalizeTranscript(wrapper.segments ?? wrapper.transcript ?? wrapper.transcripts, depth + 1);
  }
  if (!Array.isArray(input)) throw new Error('Format de transcription invalide.');
  return input.flatMap((value): TranscriptSegment[] => {
    if (!value || typeof value !== 'object' || typeof value.text !== 'string') {
      throw new Error('Segment de transcription invalide.');
    }
    // Older rows can contain the serialized enhanced transcript in their text.
    const text = value.text.trim();
    if (text.startsWith('[') || text.startsWith('{')) {
      let parsed: unknown;
      try { parsed = JSON.parse(text); }
      catch {
        if (/^\[\s*\{|^\{\s*"/.test(text)) throw new Error('Segment JSON invalide.');
      }
      // A parsed object must validate as a transcript; never fall back to JSON text.
      if (parsed && typeof parsed === 'object') return normalizeTranscript(parsed, depth + 1);
    }
    const start = value.start ?? value.audio_start_time;
    const end = value.end ?? value.audio_end_time;
    return [{
      text: value.text,
      ...(Array.isArray(value.words) ? {words: value.words} : {}),
      speaker: typeof value.speaker === 'string' && value.speaker.trim() ? value.speaker.trim() : null,
      start: typeof start === 'number' && Number.isFinite(start) && start >= 0 ? start : undefined,
      end: typeof end === 'number' && Number.isFinite(end) && end >= 0 ? end : undefined,
    }];
  });
}

export function formatElapsedTime(seconds?: number): string {
  if (seconds === undefined) return '--:--:--';
  const total = Math.floor(seconds);
  return [Math.floor(total / 3600), Math.floor(total / 60) % 60, total % 60]
    .map(part => String(part).padStart(2, '0')).join(':');
}

export function formatTranscript(input: unknown): string {
  const segments = normalizeTranscript(input);
  if (!segments.length) throw new Error('Aucune transcription à copier.');
  return segments.map(s => `[${formatElapsedTime(s.start)}] ${s.speaker ?? 'Intervenant inconnu'} : ${s.text.replace(/\s+/g, ' ').trim()}`).join('\n');
}

export function transcriptRows(input: unknown): Transcript[] {
  return normalizeTranscript(input).map((s, index) => ({
    id: `improved-${index}`, text: s.text, speaker: s.speaker ?? undefined,
    timestamp: '', sequence_id: index, audio_start_time: s.start, audio_end_time: s.end,
    duration: s.start !== undefined && s.end !== undefined ? s.end - s.start : undefined,
  }));
}
