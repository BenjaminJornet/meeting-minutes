"use client";

import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { RefreshCw, CheckCircle, XCircle, Loader2, FileAudio, Clock, HardDrive } from "lucide-react";
import { cn } from "@/lib/utils";

interface RecordingInfo {
  meeting_name: string;
  audio_path: string;
  has_transcript: boolean;
  file_size_bytes: number;
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
  };
}

export default function RetranscribePanel() {
  const [recordings, setRecordings] = useState<RecordingInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<RetranscribeProgress>({});
  const [remoteUrl, setRemoteUrl] = useState<string>("");

  // Load recordings and config on mount
  useEffect(() => {
    loadRecordings();
    loadConfig();
  }, []);

  const loadConfig = async () => {
    try {
      const config = await invoke<{ remote_whisper_url?: string } | null>("api_get_transcript_config");
      if (config?.remote_whisper_url) {
        setRemoteUrl(config.remote_whisper_url);
      }
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
      [key]: { status: "processing" },
    }));

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
            processingTime: result.processing_time_secs 
          },
        }));
        
        // Optionally save transcript to file
        // Could also trigger AI summary here
      } else {
        setProgress((prev) => ({
          ...prev,
          [key]: { 
            status: "error", 
            error: result.error || "Erreur inconnue" 
          },
        }));
      }
    } catch (err) {
      setProgress((prev) => ({
        ...prev,
        [key]: { 
          status: "error", 
          error: `${err}` 
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
                  <FileAudio className="h-5 w-5 text-gray-400 flex-shrink-0" />
                  <div className="min-w-0">
                    <p className="font-medium truncate">{recording.meeting_name}</p>
                    <div className="flex items-center gap-3 text-xs text-gray-500">
                      <span className="flex items-center gap-1">
                        <HardDrive className="h-3 w-3" />
                        {formatFileSize(recording.file_size_bytes)}
                      </span>
                      {recording.has_transcript && (
                        <span className="flex items-center gap-1 text-green-600 dark:text-green-400">
                          <CheckCircle className="h-3 w-3" />
                          Transcribed
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
                  </div>
                </div>
                <div className="flex items-center gap-2 ml-4">
                  {getStatusIcon(status)}
                  <Button
                    onClick={() => retranscribe(recording)}
                    variant="outline"
                    size="sm"
                    disabled={isProcessing}
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
    </div>
  );
}
