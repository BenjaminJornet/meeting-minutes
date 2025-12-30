import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { cn } from '@/lib/utils';
import { Check, Edit2, User } from 'lucide-react';

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

export function EnhancedTranscriptView({ content, meetingFolderPath, onTranscriptUpdate }: EnhancedTranscriptViewProps) {
  const [segments, setSegments] = useState<Segment[]>([]);
  const [editingSpeakerIndex, setEditingSpeakerIndex] = useState<number | null>(null);
  const [tempSpeakerName, setTempSpeakerName] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    try {
      if (!content) return;
      // Handle case where content might be already an object or a string
      const parsed = typeof content === 'string' ? JSON.parse(content) : content;
      // Ensure it's an array
      if (Array.isArray(parsed)) {
        setSegments(parsed);
      } else if (parsed.segments && Array.isArray(parsed.segments)) {
         // Handle case where it's wrapped in { segments: [...] }
         setSegments(parsed.segments);
      } else {
        // Fallback for simple text or unexpected format
        console.warn("Unexpected transcript format", parsed);
        setError("Invalid transcript format");
      }
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
                <button 
                  onClick={() => handleSpeakerClick(index, segment.speaker)}
                  className="text-xs font-semibold text-blue-600 hover:text-blue-800 flex items-center gap-1 transition-colors px-1.5 py-0.5 -ml-1.5 rounded hover:bg-blue-50"
                  title="Click to rename speaker"
                >
                  {segment.speaker || "Unknown Speaker"}
                  <Edit2 size={10} className="opacity-0 group-hover:opacity-100 transition-opacity text-gray-400" />
                </button>
              )}
            </div>
            <p className="text-gray-800 text-sm leading-relaxed">
              {segment.text}
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
