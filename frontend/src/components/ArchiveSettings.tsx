import { useState, useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { Archive, RefreshCw, Download, HardDrive, CheckCircle, AlertCircle, FolderOpen, ExternalLink, User } from 'lucide-react';
import { cn } from '@/lib/utils';

interface ArchivableMeeting {
  id: string;
  title: string;
  date: string;
  folder_path: string;
  is_archived: boolean;
  file_size_mb: number;
  speakers?: string[];
  archive_state?: string;
  residual_audio_count: number;
}

export function ArchiveSettings() {
  const router = useRouter();
  const [meetings, setMeetings] = useState<ArchivableMeeting[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [processingId, setProcessingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cleanupPreview, setCleanupPreview] = useState<{meeting: ArchivableMeeting; files: string[]; bytes: number} | null>(null);

  const [showAll, setShowAll] = useState(true);

  const fetchMeetings = async () => {
    setIsLoading(true);
    try {
      const data = await invoke<ArchivableMeeting[]>('get_archivable_meetings');
      
      if (showAll) {
        // Sort by date descending (newest first)
        const sorted = [...data].sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime());
        setMeetings(sorted);
      } else {
        // Filter meetings older than 1 month (approx 30 days)
        const now = new Date();
        const oneMonthAgo = new Date(now.getTime());
        oneMonthAgo.setMonth(oneMonthAgo.getMonth() - 1);
        
        const filtered = data.filter(m => {
          // Parse date YYYY-MM-DD
          const mDate = new Date(m.date);
          // If date is invalid, include the meeting (don't hide it)
          if (isNaN(mDate.getTime())) return true;
          return mDate < oneMonthAgo;
        });
        // Sort by date descending (newest first)
        const sorted = filtered.sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime());
        setMeetings(sorted);
      }
    } catch (e) {
      console.error("Failed to fetch meetings", e);
      setError("Failed to load meetings");
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchMeetings();
  }, [showAll]);

  const handleArchive = async (meeting: ArchivableMeeting) => {
    setError(null);
    setCleanupPreview(null);
    setProcessingId(meeting.id);
    try {
      const res = await invoke<{success: boolean, error?: string}>('archive_meeting', {
        meetingFolderPath: meeting.folder_path
      });
      
      if (res.success) {
        await fetchMeetings(); // Refresh list
      } else {
        setError(res.error || "Archive failed");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setProcessingId(null);
      await fetchMeetings();
    }
  };

  const previewCleanup = async (meeting: ArchivableMeeting) => {
    try {
      const inventory = await invoke<{files: string[]; bytes: number}>('get_archive_audio_inventory', { meetingFolderPath: meeting.folder_path });
      setCleanupPreview({meeting, ...inventory});
    } catch (error) { setError(String(error)); }
  };

  const handleRestore = async (meeting: ArchivableMeeting) => {
    setProcessingId(meeting.id);
    try {
      const res = await invoke<{success: boolean, error?: string}>('restore_meeting', {
        meetingFolderPath: meeting.folder_path
      });
      
      if (res.success) {
        await fetchMeetings(); // Refresh list
      } else {
        setError(res.error || "Restore failed");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setProcessingId(null);
    }
  };

  return (
    <div className="space-y-6 p-4">
      {cleanupPreview && (
        <div role="dialog" aria-label="Nettoyer les audios archivés" className="p-4 border rounded bg-blue-50 space-y-3">
          <p>Nettoyer {cleanupPreview.files.length} fichiers audio ({(cleanupPreview.bytes / 1e6).toFixed(1)} Mo) ?</p>
          <p className="text-sm">L’audio final distant sera vérifié avant toute suppression. Les transcriptions et comptes rendus sont conservés.</p>
          <ul className="text-xs max-h-40 overflow-auto">{cleanupPreview.files.map(file => <li key={file}>{file}</li>)}</ul>
          <button onClick={() => handleArchive(cleanupPreview.meeting)} className="px-3 py-2 bg-blue-600 text-white rounded">Vérifier et nettoyer</button>
          <button onClick={() => setCleanupPreview(null)} className="px-3 py-2">Annuler</button>
        </div>
      )}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-medium text-gray-900">Archive Management</h2>
          <p className="text-sm text-gray-500">
            Archive old meetings to Nextcloud to save local disk space.
          </p>
          <div className="mt-2 flex items-center gap-2">
            <input 
              type="checkbox" 
              id="showAll" 
              checked={showAll} 
              onChange={(e) => setShowAll(e.target.checked)}
              className="rounded border-gray-300 text-blue-600 focus:ring-blue-500"
            />
            <label htmlFor="showAll" className="text-xs text-gray-600 cursor-pointer select-none">
              Show all meetings (including recent ones)
            </label>
          </div>
        </div>
        <button 
          onClick={fetchMeetings} 
          className="p-2 text-gray-500 hover:bg-gray-100 rounded-full"
          title="Refresh list"
        >
          <RefreshCw size={18} className={cn(isLoading && "animate-spin")} />
        </button>
      </div>

      {error && (
        <div className="bg-red-50 text-red-600 p-3 rounded-md text-sm flex items-center gap-2">
          <AlertCircle size={16} />
          {error}
          <button onClick={() => setError(null)} className="ml-auto text-xs underline">Dismiss</button>
        </div>
      )}

      <div className="bg-white border border-gray-200 rounded-lg overflow-hidden">
        <div className="grid grid-cols-12 gap-4 p-3 bg-gray-50 border-b border-gray-200 text-xs font-medium text-gray-500 uppercase">
          <div className="col-span-5">Meeting</div>
          <div className="col-span-2">Date</div>
          <div className="col-span-2">Size</div>
          <div className="col-span-3 text-right">Action</div>
        </div>
        
        <div className="divide-y divide-gray-100 max-h-[400px] overflow-y-auto">
          {meetings.length === 0 ? (
            <div className="p-8 text-center text-gray-500 text-sm">
              {showAll ? "No meetings found." : "No meetings older than 1 month found."}
            </div>
          ) : (
            meetings.map(meeting => (
              <div key={meeting.id} className="grid grid-cols-12 gap-4 p-3 items-center hover:bg-gray-50 transition-colors">
                <div className="col-span-5 flex flex-col gap-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate font-medium text-gray-700" title={meeting.title}>
                      {meeting.title}
                    </span>
                    <button
                      onClick={() => router.push(`/meeting-details?id=${meeting.id}`)}
                      className="p-1 text-gray-400 hover:text-blue-600 hover:bg-blue-50 rounded transition-colors"
                      title="Open Meeting Details"
                    >
                      <ExternalLink size={14} />
                    </button>
                  </div>
                  <div className="flex flex-wrap gap-1">
                    <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-gray-100 text-gray-600">
                      {meeting.date}
                    </span>
                    {meeting.speakers && meeting.speakers.slice(0, 3).map((speaker, idx) => (
                      <span key={idx} className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-50 text-blue-600 gap-1">
                        <User size={10} />
                        {speaker}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="col-span-2 text-sm text-gray-500">
                  {/* Date moved to chips */}
                </div>
                <div className="col-span-2 text-sm text-gray-500 flex items-center gap-1">
                  {meeting.archive_state === 'cleanup_pending' ? (
                    <span className="text-amber-700 text-xs">Nettoyage à terminer</span>
                  ) : meeting.is_archived ? (
                    <span className="text-green-600 flex items-center gap-1 text-xs bg-green-50 px-2 py-0.5 rounded-full">
                      <CheckCircle size={10} /> Archived
                    </span>
                  ) : (
                    <span className="flex items-center gap-1">
                      <HardDrive size={12} />
                      {meeting.file_size_mb.toFixed(1)} MB
                    </span>
                  )}
                </div>
                <div className="col-span-3 text-right flex items-center justify-end gap-2">
                  {(meeting.is_archived && meeting.residual_audio_count > 0) || meeting.archive_state === 'cleanup_pending' ? (
                    <button disabled={!!processingId} onClick={() => previewCleanup(meeting)} className="px-3 py-1.5 text-xs bg-amber-50 text-amber-800 rounded">Nettoyer les audios ({meeting.residual_audio_count})</button>
                  ) : null}
                  <button
                    onClick={() => invoke('open_external_url', { url: meeting.folder_path })}
                    className="p-1.5 text-gray-500 hover:bg-gray-100 rounded-md transition-colors"
                    title="Open Folder"
                  >
                    <FolderOpen size={14} />
                  </button>
                  {meeting.is_archived ? (
                    <button
                      onClick={() => handleRestore(meeting)}
                      disabled={!!processingId}
                      className="inline-flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 rounded-md hover:bg-blue-100 disabled:opacity-50 transition-colors"
                    >
                      {processingId === meeting.id ? (
                        <RefreshCw size={12} className="animate-spin" />
                      ) : (
                        <Download size={12} />
                      )}
                      Restore
                    </button>
                  ) : (
                    <button
                      onClick={() => handleArchive(meeting)}
                      disabled={!!processingId}
                      className="inline-flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-gray-700 bg-gray-100 rounded-md hover:bg-gray-200 disabled:opacity-50 transition-colors"
                    >
                      {processingId === meeting.id ? (
                        <RefreshCw size={12} className="animate-spin" />
                      ) : (
                        <Archive size={12} />
                      )}
                      Archive
                    </button>
                  )}
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
