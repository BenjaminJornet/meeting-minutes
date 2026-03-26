"use client";

import React, { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { RefreshCw, CheckCircle, XCircle, Loader2, FileAudio, Clock, HardDrive, FolderOpen, ExternalLink, User } from "lucide-react";
import { cn } from "@/lib/utils";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { computeWeightedProgress, getProgressLabel, RETRANSCRIBE_PROGRESS_WEIGHTS } from "@/lib/task-progress";

interface RecordingInfo {
  meeting_name: string;
  audio_path: string;
  has_transcript: boolean;
  has_improved_transcript: boolean;
  file_size_bytes: number;
  has_audio: boolean;
}

interface RetranscribeResult {
  success: boolean;
  transcript: string | null;
  error: string | null;
  audio_duration_secs: number | null;
  processing_time_secs: number;
}

interface RetranscribeProgress {
  [key: string]: {
    status: "idle" | "processing" | "success" | "error";
    error?: string;
    processingTime?: number;
    phase?: string;
    phaseProgress?: number;
    overallProgress?: number;
    message?: string;
    meetingTitle?: string;
    jobId?: string;
  };
}

interface RetranscribeDialogState {
  isOpen: boolean;
  isDismissed: boolean;
  audioPath?: string;
  meetingTitle?: string;
  phase: string;
  message: string;
  phaseProgress: number;
  overallProgress: number;
  jobId?: string;
  batchIndex?: number;
  batchTotal?: number;
}

export default function RetranscribePanel() {
  const router = useRouter();
  const [recordings, setRecordings] = useState<RecordingInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<RetranscribeProgress>({});
  const [remoteUrl, setRemoteUrl] = useState<string>("");
  const [dialogState, setDialogState] = useState<RetranscribeDialogState>({
    isOpen: false,
    isDismissed: false,
    phase: 'idle',
    message: '',
    phaseProgress: 0,
    overallProgress: 0,
  });

  // Load recordings and config on mount
  useEffect(() => {
    loadRecordings();
    loadConfig();
  }, []);

  useEffect(() => {
    let cleanup: (() => void) | undefined;

    const setup = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      cleanup = await listen<any>('retranscribe-progress', (event) => {
        const payload = event.payload || {};
        const audioPath = payload.audio_path as string | undefined;
        const phase = payload.phase || 'processing';
        const phaseProgress = typeof payload.phase_progress === 'number' ? payload.phase_progress : (typeof payload.progress === 'number' ? payload.progress : 0);
        const overallProgress = typeof payload.overall_progress === 'number'
          ? payload.overall_progress
          : computeWeightedProgress(phase, phaseProgress, RETRANSCRIBE_PROGRESS_WEIGHTS);

        if (audioPath) {
          setProgress((prev) => ({
            ...prev,
            [audioPath]: {
              ...(prev[audioPath] || { status: 'processing' }),
              status: payload.status === 'error' ? 'error' : payload.status === 'complete' ? 'success' : 'processing',
              phase,
              phaseProgress,
              overallProgress,
              message: payload.message,
              meetingTitle: payload.meeting_title,
              jobId: payload.job_id,
            }
          }));
        }

        setDialogState((prev) => ({
          ...prev,
          isOpen: payload.status !== 'complete' && payload.status !== 'error' ? !prev.isDismissed : prev.isOpen,
          isDismissed: payload.status === 'complete' || payload.status === 'error' ? false : prev.isDismissed,
          audioPath: audioPath || prev.audioPath,
          meetingTitle: payload.meeting_title || prev.meetingTitle,
          phase,
          message: payload.message || prev.message,
          phaseProgress,
          overallProgress,
          jobId: payload.job_id || prev.jobId,
        }));
      });
    };

    setup();
    return () => cleanup?.();
  }, []);

  const loadConfig = async () => {
    try {
      let urlToUse = "";
      
      // 1. Get Env Var URL (defaults to localhost if not set)
      let envUrl = "";
      try {
        envUrl = await invoke<string>("api_get_backend_url");
      } catch (e) {
        console.warn("Failed to get backend URL:", e);
      }

      // 2. Get Settings URL
      let settingsUrl = "";
      try {
        const config = await invoke<{ remote_whisper_url?: string } | null>("api_get_transcript_config");
        if (config?.remote_whisper_url) {
          settingsUrl = config.remote_whisper_url;
        }
      } catch (e) {
        console.warn("Failed to get config:", e);
      }

      // 3. Decide
      // If ENV var is set to something other than localhost, it takes precedence.
      // Otherwise, if Settings are configured, use Settings.
      // Finally, fall back to whatever ENV returned (likely localhost).
      const isEnvLocal = envUrl.includes("localhost") || envUrl.includes("127.0.0.1");
      
      if (envUrl && !isEnvLocal) {
        console.log("Using ENV defined backend URL:", envUrl);
        urlToUse = envUrl;
      } else if (settingsUrl) {
        console.log("Using Settings defined URL:", settingsUrl);
        urlToUse = settingsUrl;
      } else {
        console.log("Using default/fallback URL:", envUrl);
        urlToUse = envUrl;
      }

      setRemoteUrl(urlToUse);
    } catch (err) {
      console.error("Failed to load config:", err);
    }
  };

  const loadRecordings = async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await invoke<RecordingInfo[]>("list_recordings_for_retranscription");
      setRecordings(result);
    } catch (err) {
      setError(`Erreur lors du chargement des enregistrements: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  };

  const retranscribe = async (recording: RecordingInfo) => {
    const key = recording.audio_path;
    
    setProgress((prev) => ({
      ...prev,
      [key]: { status: "processing", phase: 'preparing', phaseProgress: 0, overallProgress: 0, message: 'Preparing remote retranscription...', meetingTitle: recording.meeting_name },
    }));

    setDialogState({
      isOpen: true,
      isDismissed: false,
      audioPath: recording.audio_path,
      meetingTitle: recording.meeting_name,
      phase: 'preparing',
      message: 'Preparing remote retranscription...',
      phaseProgress: 0,
      overallProgress: 0,
    });

    try {
      const result = await invoke<RetranscribeResult>("retranscribe_audio_file", {
        audioPath: recording.audio_path,
        remoteUrl: remoteUrl || null,
        language: null, // Will use MEETILY_LANGUAGE from env
      });

      if (result.success) {
        setProgress((prev) => ({
          ...prev,
          [key]: { 
            status: "success", 
            processingTime: result.processing_time_secs,
            phase: 'completed',
            phaseProgress: 100,
            overallProgress: 100,
            message: 'Re-transcription completed successfully.',
            meetingTitle: recording.meeting_name,
          },
        }));
        setDialogState(prev => ({ ...prev, meetingTitle: recording.meeting_name, phase: 'completed', message: 'Re-transcription completed successfully.', phaseProgress: 100, overallProgress: 100, isOpen: false, isDismissed: false }));
        
        // Optionally save transcript to file
        // Could also trigger AI summary here
      } else {
        setProgress((prev) => ({
          ...prev,
          [key]: { 
            status: "error", 
            error: result.error || "Erreur inconnue",
            phase: 'error',
            message: result.error || "Erreur inconnue",
            meetingTitle: recording.meeting_name,
          },
        }));
      }
    } catch (err) {
      setProgress((prev) => ({
        ...prev,
        [key]: { 
          status: "error", 
            error: `${err}`,
            phase: 'error',
            message: `${err}`,
            meetingTitle: recording.meeting_name,
        },
      }));
    }
  };

  const retranscribeAll = async () => {
    for (const recording of recordings) {
      if (progress[recording.audio_path]?.status !== "processing") {
        await retranscribe(recording);
      }
    }
  };

  const getStatusIcon = (status: RetranscribeProgress[string] | undefined) => {
    if (!status || status.status === "idle") {
      return null;
    }
    switch (status.status) {
      case "processing":
        return <Loader2 className="h-4 w-4 animate-spin text-blue-500" />;
      case "success":
        return <CheckCircle className="h-4 w-4 text-green-500" />;
      case "error":
        return <XCircle className="h-4 w-4 text-red-500" />;
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center p-8">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        <span className="ml-2 text-muted-foreground">Loading recordings...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="border border-red-200 dark:border-red-800 rounded-lg p-6">
        <div className="flex items-center text-red-600 dark:text-red-400">
          <XCircle className="h-5 w-5 mr-2" />
          {error}
        </div>
        <Button onClick={loadRecordings} variant="outline" className="mt-4">
          <RefreshCw className="h-4 w-4 mr-2" />
          Retry
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <FileAudio className="h-5 w-5" />
            Re-transcription with Remote Server
          </h3>
          <p className="text-sm text-gray-500 mt-1">
            Use your GPU Whisper server to get higher quality transcriptions with speaker identification
          </p>
        </div>
        <div className="flex gap-2">
          <Button onClick={loadRecordings} variant="outline" size="sm">
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
          {recordings.length > 0 && (
            <Button onClick={retranscribeAll} size="sm">
              <RefreshCw className="h-4 w-4 mr-2" />
              Re-transcribe All
            </Button>
          )}
        </div>
      </div>

      {recordings.length === 0 ? (
        <p className="text-gray-500 text-center py-8">
          No recordings found
        </p>
      ) : (
        <div className="space-y-3">
          {recordings.map((recording) => {
            const status = progress[recording.audio_path];
            const isProcessing = status?.status === "processing";

            return (
              <div
                key={recording.audio_path}
                className={cn(
                  "flex items-center justify-between p-3 rounded-lg border",
                  status?.status === "success" && "border-green-200 dark:border-green-800 bg-green-50 dark:bg-green-950",
                  status?.status === "error" && "border-red-200 dark:border-red-800 bg-red-50 dark:bg-red-950",
                  !status && "border-gray-200 dark:border-gray-700"
                )}
              >
                <div className="flex items-center gap-3 flex-1 min-w-0">
                  <FileAudio className={cn("h-5 w-5 flex-shrink-0", recording.has_audio ? "text-gray-400" : "text-orange-400")} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <p className="font-medium truncate">{recording.meeting_name}</p>
                      <button
                        onClick={() => router.push(`/meeting-details?id=${recording.meeting_name}`)}
                        className="p-1 text-gray-400 hover:text-blue-600 hover:bg-blue-50 rounded transition-colors"
                        title="Open Meeting Details"
                      >
                        <ExternalLink size={14} />
                      </button>
                    </div>
                    <div className="flex flex-wrap gap-2 text-xs text-gray-500 mt-1">
                      <span className="flex items-center gap-1">
                        <HardDrive className="h-3 w-3" />
                        {recording.has_audio ? formatFileSize(recording.file_size_bytes) : 'Audio archivé'}
                      </span>
                      {recording.has_transcript && (
                        <span className="flex items-center gap-1 text-green-600 dark:text-green-400 bg-green-50 px-1.5 py-0.5 rounded">
                          <CheckCircle className="h-3 w-3" />
                          Transcribed
                        </span>
                      )}
                      {recording.has_improved_transcript && (
                        <span className="flex items-center gap-1 text-blue-600 dark:text-blue-400 bg-blue-50 px-1.5 py-0.5 rounded">
                          <CheckCircle className="h-3 w-3" />
                          Enhanced
                        </span>
                      )}
                      {/* Attempt to parse date from meeting name if it matches pattern Meeting YYYY-MM-DD... */}
                      {recording.meeting_name.match(/Meeting \d{4}-\d{2}-\d{2}/) && (
                        <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-gray-100 text-gray-600">
                          {recording.meeting_name.split('_')[0].replace('Meeting ', '')}
                        </span>
                      )}
                      {status?.processingTime && (
                        <span className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {status.processingTime.toFixed(1)}s
                        </span>
                      )}
                    </div>
                    {status?.error && (
                      <p className="text-xs text-red-600 dark:text-red-400 mt-1">
                        {status.error}
                      </p>
                    )}
                    {isProcessing && (
                      <div className="mt-2 space-y-1">
                        <div className="h-1.5 rounded-full bg-gray-100 overflow-hidden">
                          <div className="h-full bg-blue-600 transition-all duration-500" style={{ width: `${Math.max(2, status?.overallProgress || 0)}%` }} />
                        </div>
                        <p className="text-xs text-blue-600">
                          {status?.message || 'Processing...'} · {Math.round(status?.overallProgress || 0)}%
                        </p>
                      </div>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-2 ml-4">
                  <Button
                    onClick={() => {
                      // Extract folder path from audio path
                      const folderPath = recording.audio_path.substring(0, Math.max(recording.audio_path.lastIndexOf('/'), recording.audio_path.lastIndexOf('\\')));
                      invoke('open_external_url', { url: folderPath });
                    }}
                    variant="ghost"
                    size="icon"
                    title="Open Folder"
                  >
                    <FolderOpen className="h-4 w-4" />
                  </Button>
                  {getStatusIcon(status)}
                  <Button
                    onClick={() => retranscribe(recording)}
                    variant="outline"
                    size="sm"
                    disabled={isProcessing || !recording.has_audio}
                    title={!recording.has_audio ? "Audio file not available (archived or missing)" : "Re-transcribe this recording"}
                  >
                    {isProcessing ? (
                      <>
                        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                        Processing...
                      </>
                    ) : (
                      <>
                        <RefreshCw className="h-4 w-4 mr-2" />
                        Re-transcribe
                      </>
                    )}
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {remoteUrl && (
        <div className="p-3 bg-gray-100 dark:bg-gray-800 rounded-lg">
          <p className="text-sm text-gray-600 dark:text-gray-400">
            <strong>Remote server:</strong> {remoteUrl}
          </p>
        </div>
      )}

      <Dialog open={dialogState.isOpen} onOpenChange={(open) => {
        setDialogState(prev => ({
          ...prev,
          isOpen: open,
          isDismissed: !open && prev.phase !== 'completed' && prev.phase !== 'error',
        }));
      }}>
        <DialogContent className="sm:max-w-[520px]">
          <DialogTitle>Re-transcription in progress</DialogTitle>
          <div className="space-y-4 py-2">
            <div>
              <div className="text-sm font-medium text-gray-900">{dialogState.meetingTitle || 'Remote retranscription'}</div>
              <div className="text-sm text-gray-500 mt-1">{dialogState.message}</div>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span className="font-medium text-blue-700">{getProgressLabel(dialogState.phase)}</span>
                <span className="text-gray-500">{Math.round(dialogState.overallProgress)}%</span>
              </div>
              <div className="h-2 rounded-full bg-gray-100 overflow-hidden">
                <div className="h-full bg-blue-600 rounded-full transition-all duration-500" style={{ width: `${Math.max(2, dialogState.overallProgress)}%` }} />
              </div>
              <div className="flex items-center justify-between text-xs text-gray-500">
                <span>Step progress</span>
                <span>{Math.round(dialogState.phaseProgress)}%</span>
              </div>
            </div>

            <div className="rounded-lg border border-blue-100 bg-blue-50 p-3 text-sm text-blue-800">
              The app is sending your recording to the remote backend, tracking the remote job, then saving the enhanced transcript locally.
            </div>

            {dialogState.jobId && (
              <div className="text-xs text-gray-500 break-all">Job ID: {dialogState.jobId}</div>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
