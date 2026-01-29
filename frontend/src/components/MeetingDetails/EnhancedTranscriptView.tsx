import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { cn } from '@/lib/utils';
import { Check, Edit2, User, Search, X } from 'lucide-react';

interface Word {
  word: string;
  start: number;
  end: number;
  probability: number;
}

interface Segment {
  text: string;
  speaker: string | null;
  start: number;
  end: number;
  words: Word[];
}

interface EnhancedTranscriptViewProps {
  content: string;
  meetingFolderPath?: string | null;
  onTranscriptUpdate?: () => void;
}

// Fixed color palette for speakers by index - distinct, accessible colors
const SPEAKER_COLORS = [
  { bg: 'bg-blue-50', text: 'text-blue-700', border: 'border-blue-200' },
  { bg: 'bg-green-50', text: 'text-green-700', border: 'border-green-200' },
  { bg: 'bg-purple-50', text: 'text-purple-700', border: 'border-purple-200' },
  { bg: 'bg-orange-50', text: 'text-orange-700', border: 'border-orange-200' },
  { bg: 'bg-pink-50', text: 'text-pink-700', border: 'border-pink-200' },
  { bg: 'bg-teal-50', text: 'text-teal-700', border: 'border-teal-200' },
  { bg: 'bg-indigo-50', text: 'text-indigo-700', border: 'border-indigo-200' },
  { bg: 'bg-red-50', text: 'text-red-700', border: 'border-red-200' },
];

export function EnhancedTranscriptView({ content, meetingFolderPath, onTranscriptUpdate }: EnhancedTranscriptViewProps) {
  const [segments, setSegments] = useState<Segment[]>([]);
  const [editingSpeakerIndex, setEditingSpeakerIndex] = useState<number | null>(null);
  const [tempSpeakerName, setTempSpeakerName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [speakerColorMap, setSpeakerColorMap] = useState<Map<string, number>>(new Map());
  
  // Search state
  const [searchQuery, setSearchQuery] = useState('');
  const [showSearch, setShowSearch] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Handle Ctrl+F for search
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
        e.preventDefault();
        setShowSearch(true);
        setTimeout(() => searchInputRef.current?.focus(), 50);
      }
      if (e.key === 'Escape' && showSearch) {
        setShowSearch(false);
        setSearchQuery('');
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showSearch]);

  // Helper to highlight search matches in text
  const highlightText = useCallback((text: string): React.ReactNode => {
    if (!searchQuery.trim()) return text;
    
    const regex = new RegExp(`(${searchQuery.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi');
    const parts = text.split(regex);
    
    return parts.map((part, i) => 
      regex.test(part) 
        ? <mark key={i} className="bg-yellow-300 rounded px-0.5">{part}</mark>
        : part
    );
  }, [searchQuery]);

  useEffect(() => {
    try {
      if (!content) return;
      // Handle case where content might be already an object or a string
      const parsed = typeof content === 'string' ? JSON.parse(content) : content;
      let parsedSegments: Segment[] = [];
      // Ensure it's an array
      if (Array.isArray(parsed)) {
        parsedSegments = parsed;
      } else if (parsed.segments && Array.isArray(parsed.segments)) {
         // Handle case where it's wrapped in { segments: [...] }
         parsedSegments = parsed.segments;
      } else {
        // Fallback for simple text or unexpected format
        console.warn("Unexpected transcript format", parsed);
        setError("Invalid transcript format");
        return;
      }
      
      setSegments(parsedSegments);
      
      // Build speaker color map based on order of appearance
      const colorMap = new Map<string, number>();
      let colorIndex = 0;
      parsedSegments.forEach(segment => {
        const speaker = segment.speaker || 'Unknown Speaker';
        if (!colorMap.has(speaker)) {
          colorMap.set(speaker, colorIndex % SPEAKER_COLORS.length);
          colorIndex++;
        }
      });
      setSpeakerColorMap(colorMap);
    } catch (e) {
      console.error("Failed to parse enhanced transcript JSON", e);
      setError("Failed to parse transcript data");
    }
  }, [content]);

  const handleSpeakerClick = (index: number, currentSpeaker: string | null) => {
    setEditingSpeakerIndex(index);
    setTempSpeakerName(currentSpeaker || '');
  };

  const handleSpeakerSave = async (index: number) => {
    const oldSpeakerName = segments[index].speaker;
    const newSpeakerName = tempSpeakerName || null;
    
    // Update ALL segments that had the same speaker name
    const newSegments = segments.map(segment => {
      // If the segment has the same speaker as the one we're editing (or both are null/undefined)
      if (segment.speaker === oldSpeakerName) {
        return { ...segment, speaker: newSpeakerName };
      }
      return segment;
    });

    setSegments(newSegments);
    setEditingSpeakerIndex(null);

    // Save to file if possible
    if (meetingFolderPath) {
      try {
        // Construct audio path (assumed to be audio.mp4 in the meeting folder)
        // We use forward slashes which usually work on Windows in Rust PathBuf too, 
        // or we rely on the backend to handle it.
        // Ideally we should use the join API, but for now string concatenation with / is a safe bet for web-style paths
        // that Rust's PathBuf::from handles well on Windows too.
        const audioPath = `${meetingFolderPath}/audio.mp4`; 
        
        await invoke('save_improved_transcript', {
          audioPath: audioPath,
          jsonContent: JSON.stringify(newSegments)
        });
        
        // Notify parent to refresh data
        if (onTranscriptUpdate) {
          onTranscriptUpdate();
        }
      } catch (e) {
        console.error("Failed to save transcript", e);
      }
    }
  };

  if (error) {
    return (
      <div className="p-4 text-red-500 text-sm">
        <p>Error displaying enhanced transcript.</p>
        <pre className="mt-2 text-xs bg-gray-100 p-2 rounded overflow-auto max-h-40">
          {content}
        </pre>
      </div>
    );
  }

  return (
    <div className="space-y-4 p-4">
      {/* Search bar - shows on Ctrl+F */}
      {showSearch && (
        <div className="sticky top-0 z-10 bg-white pb-2">
          <div className="relative flex items-center gap-2">
            <Search size={16} className="absolute left-3 text-gray-400" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search transcript..."
              className="w-full pl-9 pr-8 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              autoFocus
            />
            <button
              onClick={() => { setShowSearch(false); setSearchQuery(''); }}
              className="absolute right-3 text-gray-400 hover:text-gray-600"
              title="Close search (Esc)"
            >
              <X size={16} />
            </button>
          </div>
          {searchQuery && (
            <div className="mt-1 text-xs text-gray-500">
              {segments.filter(s => s.text.toLowerCase().includes(searchQuery.toLowerCase())).length} matches found
            </div>
          )}
        </div>
      )}
      
      <div className="text-xs text-green-600 mb-4 flex items-center gap-1 bg-green-50 p-2 rounded border border-green-100">
        <span>✨</span>
        <span>Enhanced transcript from GPU server</span>
        <span className="text-gray-400 mx-2">|</span>
        <span className="text-gray-500">Click speaker names to edit</span>
      </div>
      
      {segments.map((segment, index) => (
        <div key={index} className="flex gap-3 group">
          <div className="w-12 pt-1 text-xs text-gray-400 font-mono shrink-0 text-right">
            {formatTime(segment.start)}
          </div>
          <div className="flex-1">
            <div className="flex items-center gap-2 mb-1">
              {editingSpeakerIndex === index ? (
                <div className="flex items-center gap-1">
                  <input
                    type="text"
                    value={tempSpeakerName}
                    onChange={(e) => setTempSpeakerName(e.target.value)}
                    className="text-xs border border-blue-300 rounded px-2 py-1 w-40 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    placeholder="Speaker Name"
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleSpeakerSave(index);
                      if (e.key === 'Escape') setEditingSpeakerIndex(null);
                    }}
                    onClick={(e) => e.stopPropagation()}
                  />
                  <button 
                    onClick={(e) => { e.stopPropagation(); handleSpeakerSave(index); }} 
                    className="p-1 text-green-600 hover:bg-green-50 rounded"
                    title="Save"
                  >
                    <Check size={14} />
                  </button>
                </div>
              ) : (
                (() => {
                  const speakerName = segment.speaker || 'Unknown Speaker';
                  const colorIdx = speakerColorMap.get(speakerName) ?? 0;
                  const colors = SPEAKER_COLORS[colorIdx];
                  return (
                    <button 
                      onClick={() => handleSpeakerClick(index, segment.speaker)}
                      className={cn(
                        "text-xs font-semibold flex items-center gap-1 transition-colors px-1.5 py-0.5 -ml-1.5 rounded",
                        colors.bg, colors.text, `hover:${colors.border}`
                      )}
                      title="Click to rename speaker"
                    >
                      {speakerName}
                      <Edit2 size={10} className="opacity-0 group-hover:opacity-100 transition-opacity text-gray-400" />
                    </button>
                  );
                })()
              )}
            </div>
            <p className="text-gray-800 text-sm leading-relaxed">
              {highlightText(segment.text)}
            </p>
          </div>
        </div>
      ))}
    </div>
  );
}

function formatTime(seconds: number | undefined): string {
  if (seconds === undefined) return "--:--";
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}
