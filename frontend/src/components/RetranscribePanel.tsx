"use client";

import React, { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { RefreshCw, CheckCircle, XCircle, Loader2, FileAudio, Clock, HardDrive, FolderOpen, ExternalLink, User } from "lucide-react";
import { cn } from "@/lib/utils";

interface RecordingInfo {
  meeting_name: string;
  audio_path: string;
  has_transcript: boolean;
  has_improved_transcript: boolean;
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
  const router = useRouter();
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
                        {formatFileSize(recording.file_size_bytes)}
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
