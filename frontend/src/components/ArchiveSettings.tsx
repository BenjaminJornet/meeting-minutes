import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Archive, RefreshCw, Download, HardDrive, CheckCircle, AlertCircle } from 'lucide-react';
import { cn } from '@/lib/utils';

interface ArchivableMeeting {
  id: string;
  title: string;
  date: string;
  folder_path: string;
  is_archived: boolean;
  file_size_mb: number;
}

export function ArchiveSettings() {
  const [meetings, setMeetings] = useState<ArchivableMeeting[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [processingId, setProcessingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchMeetings = async () => {
    setIsLoading(true);
    try {
      const data = await invoke<ArchivableMeeting[]>('get_archivable_meetings');
      // Filter meetings older than 1 month (approx 30 days)
      // Or just show all for now as requested "montrant les meetings de plus de 1 mois"
      // Let's filter client side for flexibility
      const now = new Date();
      const oneMonthAgo = new Date(now.setMonth(now.getMonth() - 1));
      
      const filtered = data.filter(m => {
        // Parse date YYYY-MM-DD
        const mDate = new Date(m.date);
        return mDate < oneMonthAgo;
      });
      
      setMeetings(filtered);
    } catch (e) {
      console.error("Failed to fetch meetings", e);
      setError("Failed to load meetings");
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchMeetings();
  }, []);

  const handleArchive = async (meeting: ArchivableMeeting) => {
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
    }
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
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-medium text-gray-900">Archive Management</h2>
          <p className="text-sm text-gray-500">
            Archive old meetings to Nextcloud to save local disk space.
            Only meetings older than 1 month are shown.
          </p>
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
              No meetings older than 1 month found.
            </div>
          ) : (
            meetings.map(meeting => (
              <div key={meeting.id} className="grid grid-cols-12 gap-4 p-3 items-center hover:bg-gray-50 transition-colors">
                <div className="col-span-5 truncate font-medium text-gray-700" title={meeting.title}>
                  {meeting.title}
                </div>
                <div className="col-span-2 text-sm text-gray-500">
                  {meeting.date}
                </div>
                <div className="col-span-2 text-sm text-gray-500 flex items-center gap-1">
                  {meeting.is_archived ? (
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
                <div className="col-span-3 text-right">
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
