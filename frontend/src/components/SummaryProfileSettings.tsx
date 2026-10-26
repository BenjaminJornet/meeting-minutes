import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

interface Options { contextTokens: number; maxChunkChars: number; outputTokens: number }
const defaults: Options = {contextTokens: 32768, maxChunkChars: 30000, outputTokens: 4096};

export function SummaryProfileSettings({endpoint, provider, model}: {endpoint?: string | null; provider: string; model: string}) {
  const [options, setOptions] = useState<Options>(defaults);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  useEffect(() => {
    let current = true;
    setLoading(true); setError('');
    invoke<Options>('api_get_summary_options', {endpoint: endpoint || null, provider, model})
      .then(value => { if (current) setOptions(value); })
      .catch(reason => { if (current) setError(String(reason)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [endpoint, provider, model]);
  const save = async () => {
    setLoading(true);
    try {
      await invoke('api_save_summary_options', {endpoint: endpoint || null, provider, model, options});
      setError(''); toast.success('Réglages du compte rendu enregistrés');
    } catch (reason) { setError(String(reason)); }
    finally { setLoading(false); }
  };
  return (
    <fieldset className="mt-4 p-3 border rounded space-y-3" disabled={loading}>
      <legend className="text-sm font-medium">Comptes rendus longs — {model}</legend>
      <p className="text-xs text-gray-500">Réglages conservés pour ce serveur et ce modèle. Les morceaux sont réduits automatiquement pour réserver la place aux instructions et à la réponse.</p>
      {([
        ['contextTokens', 'Contexte demandé (tokens)', 2048, 262144],
        ['maxChunkChars', 'Taille maximale d’un morceau (caractères)', 1000, 200000],
        ['outputTokens', 'Budget de réponse (tokens)', 256, 32768],
      ] as const).map(([key, label, min, max]) => (
        <label key={key} className="block text-sm">{label}
          <input className="mt-1 block w-full p-2 border rounded" type="number" min={min} max={max}
            value={options[key]} onChange={event => setOptions({...options, [key]: Number(event.target.value)})} />
        </label>
      ))}
      {error && <p role="alert" className="text-sm text-red-600">{error}</p>}
      <button type="button" className="px-3 py-2 border rounded text-sm" onClick={save}>Enregistrer ces réglages</button>
    </fieldset>
  );
}
