"use client";

import { useState } from 'react';
import { Transcript } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { cn } from '@/lib/utils';
import { EnhancedTranscriptView } from './EnhancedTranscriptView';
import { LoaderIcon } from 'lucide-react';

interface TranscriptPanelProps {
  transcripts: Transcript[];
  improvedTranscript?: string | null;
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  meetingFolderPath?: string | null;
  isWaitingForEnhanced?: boolean;
  retranscriptionProgress?: number | null;
  onTranscriptUpdate?: () => void;
  activeSpeakerFilter?: string | null;
}

export function TranscriptPanel({
  transcripts,
  improvedTranscript,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  meetingFolderPath,
  isWaitingForEnhanced,
  retranscriptionProgress,
  onTranscriptUpdate,
  activeSpeakerFilter
}: TranscriptPanelProps) {
  const [showImproved, setShowImproved] = useState(!!improvedTranscript);
  const hasImprovedTranscript = !!improvedTranscript && improvedTranscript.trim().length > 0;

  return (
    <div className="hidden md:flex md:w-1/4 lg:w-1/3 min-w-0 border-r border-gray-200 bg-white flex-col relative shrink-0">
      {/* Title area */}
      <div className="p-4 border-b border-gray-200">
        <div className="flex items-center justify-between">
          {/* Toggle between improved and original transcripts */}
          {hasImprovedTranscript ? (
            <div className="flex items-center gap-2">
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
          ) : (
            <div />
          )}

          <div className="flex-shrink-0">
            <TranscriptButtonGroup
              transcriptCount={transcripts?.length || 0}
              onCopyTranscript={onCopyTranscript}
              onOpenMeetingFolder={onOpenMeetingFolder}
            />
          </div>
        </div>

        {/* Progress Indicator */}
        {isWaitingForEnhanced && (
          <div className="mt-3 p-3 bg-blue-50 rounded-md border border-blue-100">
            <div className="flex items-center gap-2 mb-2">
              <LoaderIcon className="size-4 animate-spin text-blue-600" />
              <span className="text-sm font-medium text-blue-700">
                Enhancing transcript...
              </span>
              {retranscriptionProgress !== null && retranscriptionProgress !== undefined && (
                <span className="ml-auto text-xs font-medium text-blue-600">
                  {Math.round(retranscriptionProgress)}%
                </span>
              )}
            </div>
            {retranscriptionProgress !== null && retranscriptionProgress !== undefined && (
              <div className="w-full bg-blue-200 rounded-full h-1.5">
                <div
                  className="bg-blue-600 h-1.5 rounded-full transition-all duration-300 ease-out"
                  style={{ width: `${Math.max(5, Math.min(100, retranscriptionProgress))}%` }}
                />
              </div>
            )}
          </div>
        )}
      </div>

      {/* Active speaker filter banner */}
      {activeSpeakerFilter && (
        <div className="sticky top-0 z-10 bg-blue-600 text-white px-4 py-2 flex items-center justify-between text-sm">
          <span>Filtre actif : <strong>{activeSpeakerFilter}</strong></span>
          <span className="text-xs opacity-75">Cliquez sur le chip pour désactiver</span>
        </div>
      )}

      {/* Transcript content */}
      <div className="flex-1 overflow-y-auto pb-4">
        {showImproved && hasImprovedTranscript ? (
          <EnhancedTranscriptView
            content={improvedTranscript!}
            meetingFolderPath={meetingFolderPath}
            onTranscriptUpdate={onTranscriptUpdate}
            activeSpeakerFilter={activeSpeakerFilter}
          />
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
