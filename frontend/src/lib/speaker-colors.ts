// Shared speaker color palette used across transcript and summary views
// Colors are assigned by order of appearance and cycle through the palette

export const SPEAKER_COLORS = [
  { bg: 'bg-blue-50', text: 'text-blue-700', border: 'border-blue-200', chipBg: 'bg-blue-50', chipText: 'text-blue-700', chipBorder: 'border-blue-100', activeBg: 'bg-blue-600', activeText: 'text-white' },
  { bg: 'bg-green-50', text: 'text-green-700', border: 'border-green-200', chipBg: 'bg-green-50', chipText: 'text-green-700', chipBorder: 'border-green-100', activeBg: 'bg-green-600', activeText: 'text-white' },
  { bg: 'bg-purple-50', text: 'text-purple-700', border: 'border-purple-200', chipBg: 'bg-purple-50', chipText: 'text-purple-700', chipBorder: 'border-purple-100', activeBg: 'bg-purple-600', activeText: 'text-white' },
  { bg: 'bg-orange-50', text: 'text-orange-700', border: 'border-orange-200', chipBg: 'bg-orange-50', chipText: 'text-orange-700', chipBorder: 'border-orange-100', activeBg: 'bg-orange-600', activeText: 'text-white' },
  { bg: 'bg-pink-50', text: 'text-pink-700', border: 'border-pink-200', chipBg: 'bg-pink-50', chipText: 'text-pink-700', chipBorder: 'border-pink-100', activeBg: 'bg-pink-600', activeText: 'text-white' },
  { bg: 'bg-teal-50', text: 'text-teal-700', border: 'border-teal-200', chipBg: 'bg-teal-50', chipText: 'text-teal-700', chipBorder: 'border-teal-100', activeBg: 'bg-teal-600', activeText: 'text-white' },
  { bg: 'bg-indigo-50', text: 'text-indigo-700', border: 'border-indigo-200', chipBg: 'bg-indigo-50', chipText: 'text-indigo-700', chipBorder: 'border-indigo-100', activeBg: 'bg-indigo-600', activeText: 'text-white' },
  { bg: 'bg-red-50', text: 'text-red-700', border: 'border-red-200', chipBg: 'bg-red-50', chipText: 'text-red-700', chipBorder: 'border-red-100', activeBg: 'bg-red-600', activeText: 'text-white' },
] as const;

export type SpeakerColor = typeof SPEAKER_COLORS[number];

/**
 * Build a map of speaker name → color index based on order of appearance in transcripts.
 * The same speaker always gets the same color within a session.
 */
export function buildSpeakerColorMap(speakers: string[]): Map<string, number> {
  const colorMap = new Map<string, number>();
  let colorIndex = 0;
  for (const speaker of speakers) {
    if (!colorMap.has(speaker)) {
      colorMap.set(speaker, colorIndex % SPEAKER_COLORS.length);
      colorIndex++;
    }
  }
  return colorMap;
}
