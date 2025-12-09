"use client";

import { useState } from 'react';
import { Transcript } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { cn } from '@/lib/utils';

interface TranscriptPanelProps {
  transcripts: Transcript[];
  improvedTranscript?: string | null;
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
}

export function TranscriptPanel({
  transcripts,
  improvedTranscript,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording
}: TranscriptPanelProps) {
  const [showImproved, setShowImproved] = useState(!!improvedTranscript);
  const hasImprovedTranscript = !!improvedTranscript && improvedTranscript.trim().length > 0;

  return (
    <div className="hidden md:flex md:w-1/4 lg:w-1/3 min-w-0 border-r border-gray-200 bg-white flex-col relative shrink-0">
      {/* Title area */}
      <div className="p-4 border-b border-gray-200">
        <TranscriptButtonGroup
          transcriptCount={transcripts?.length || 0}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
        />
        
        {/* Toggle between improved and original transcripts */}
        {hasImprovedTranscript && (
          <div className="mt-3 flex items-center gap-2">
            <button
              onClick={() => setShowImproved(true)}
              className={cn(
                "px-3 py-1.5 text-xs font-medium rounded-md transition-colors",
                showImproved
                  ? "bg-blue-600 text-white"
                  : "bg-gray-100 text-gray-600 hover:bg-gray-200"
              )}
            >
              ✨ Enhanced
            </button>
            <button
              onClick={() => setShowImproved(false)}
              className={cn(
                "px-3 py-1.5 text-xs font-medium rounded-md transition-colors",
                !showImproved
                  ? "bg-blue-600 text-white"
                  : "bg-gray-100 text-gray-600 hover:bg-gray-200"
              )}
            >
              📝 Original
            </button>
          </div>
        )}
      </div>

      {/* Transcript content */}
      <div className="flex-1 overflow-y-auto pb-4">
        {showImproved && hasImprovedTranscript ? (
          <div className="p-4">
            <div className="text-xs text-green-600 mb-2 flex items-center gap-1">
              <span>✨</span>
              <span>Enhanced transcript from GPU server</span>
            </div>
            <div className="prose prose-sm max-w-none">
              {improvedTranscript.split('\n').map((line, index) => (
                <p key={index} className="mb-2 text-gray-700">
                  {line}
                </p>
              ))}
            </div>
          </div>
        ) : (
          <TranscriptView transcripts={transcripts} />
        )}
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && transcripts.length > 0 && (
        <div className="p-1 border-t border-gray-200">
          <textarea
            placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
            className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
