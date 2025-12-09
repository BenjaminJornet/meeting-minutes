import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock, Globe, Monitor } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'remoteWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
    remoteUrl?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

// Helper to save config to backend
async function saveTranscriptConfig(config: TranscriptModelProps) {
    try {
        console.log('[TranscriptSettings] Auto-saving config:', config);
        await invoke('api_save_transcript_config', {
            provider: config.provider,
            model: config.model,
            apiKey: config.apiKey,
            remoteUrl: config.remoteUrl
        });
        console.log('[TranscriptSettings] ✅ Config saved');
    } catch (error) {
        console.error('[TranscriptSettings] ❌ Failed to save config:', error);
    }
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [selectedWhisperModel, setSelectedWhisperModel] = useState<string>(transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : 'small');
    const [selectedParakeetModel, setSelectedParakeetModel] = useState<string>(transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : 'parakeet-tdt-0.6b-v3-int8');
    const [remoteWhisperUrl, setRemoteWhisperUrl] = useState<string>(transcriptModelConfig.remoteUrl || 'http://localhost:8178');
    const [isCheckingRemote, setIsCheckingRemote] = useState<boolean>(false);
    const [remoteStatus, setRemoteStatus] = useState<'unknown' | 'connected' | 'error'>('unknown');
    const [hasLoadedEnvDefaults, setHasLoadedEnvDefaults] = useState<boolean>(false);

    // Load default values from .env for remote whisper URL
    useEffect(() => {
        const loadEnvDefaults = async () => {
            if (hasLoadedEnvDefaults) return;
            
            try {
                const envDefaults = await invoke('get_env_defaults') as {
                    ollama_url: string | null;
                    whisper_url: string | null;
                    backend_url: string | null;
                    language: string | null;
                };
                
                // If we have an env URL and no saved config URL, use the env URL
                if (envDefaults.whisper_url) {
                    // Always prefer the env URL if no remoteUrl is saved in config
                    if (!transcriptModelConfig.remoteUrl || transcriptModelConfig.remoteUrl === 'http://localhost:8178') {
                        console.log('Loading Remote Whisper URL from .env:', envDefaults.whisper_url);
                        setRemoteWhisperUrl(envDefaults.whisper_url);
                        // Also update the config so it gets saved
                        setTranscriptModelConfig({
                            ...transcriptModelConfig,
                            remoteUrl: envDefaults.whisper_url
                        });
                    }
                }
                setHasLoadedEnvDefaults(true);
            } catch (err) {
                console.error('Failed to load env defaults:', err);
                setHasLoadedEnvDefaults(true);
            }
        };
        
        loadEnvDefaults();
    }, [hasLoadedEnvDefaults]);

    // Sync remoteWhisperUrl when config changes
    useEffect(() => {
        if (transcriptModelConfig.remoteUrl) {
            setRemoteWhisperUrl(transcriptModelConfig.remoteUrl);
        }
    }, [transcriptModelConfig.remoteUrl]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet' || transcriptModelConfig.provider === 'remoteWhisper') {
            setApiKey(null);
        }
        // Check remote status when remoteWhisper is selected
        if (transcriptModelConfig.provider === 'remoteWhisper') {
            checkRemoteWhisperStatus();
        }
    }, [transcriptModelConfig.provider]);

    const checkRemoteWhisperStatus = async () => {
        setIsCheckingRemote(true);
        try {
            const result = await invoke('check_remote_whisper_status', { url: remoteWhisperUrl }) as boolean;
            setRemoteStatus(result ? 'connected' : 'error');
        } catch (err) {
            console.error('Error checking remote whisper:', err);
            setRemoteStatus('error');
        }
        setIsCheckingRemote(false);
    };

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKey(null);
        }
    };
    const modelOptions = {
        localWhisper: [selectedWhisperModel],
        remoteWhisper: ['large-v3', 'large-v2', 'medium', 'small', 'base', 'tiny'],
        parakeet: [selectedParakeetModel],
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';
    const isRemoteWhisper = transcriptModelConfig.provider === 'remoteWhisper';

    const handleInputClick = () => {
        if (isApiKeyLocked) {
            setIsLockButtonVibrating(true);
            setTimeout(() => setIsLockButtonVibrating(false), 500);
        }
    };

    const handleWhisperModelSelect = (modelName: string) => {
        setSelectedWhisperModel(modelName);
        if (transcriptModelConfig.provider === 'localWhisper') {
            setTranscriptModelConfig({
                ...transcriptModelConfig,
                model: modelName
            });
            // Close modal after selection
            if (onModelSelect) {
                onModelSelect();
            }
        }
    };

    const handleParakeetModelSelect = (modelName: string) => {
        setSelectedParakeetModel(modelName);
        if (transcriptModelConfig.provider === 'parakeet') {
            setTranscriptModelConfig({
                ...transcriptModelConfig,
                model: modelName
            });
            // Close modal after selection
            if (onModelSelect) {
                onModelSelect();
            }
        }
    };

    return (
        <div className='max-h-[calc(100vh-200px)]'>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            Transcript Model
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={transcriptModelConfig.provider}
                                onValueChange={(value) => {
                                    const provider = value as TranscriptModelProps['provider'];
                                    const newModel = provider === 'localWhisper' ? selectedWhisperModel : 
                                                    provider === 'parakeet' ? selectedParakeetModel :
                                                    provider === 'remoteWhisper' ? 'large-v3' :
                                                    modelOptions[provider][0];
                                    // Preserve remoteUrl when switching providers
                                    const newConfig = { 
                                        ...transcriptModelConfig, 
                                        provider, 
                                        model: newModel,
                                        remoteUrl: provider === 'remoteWhisper' ? (remoteWhisperUrl || transcriptModelConfig.remoteUrl) : transcriptModelConfig.remoteUrl
                                    };
                                    setTranscriptModelConfig(newConfig);
                                    // Auto-save when provider changes
                                    saveTranscriptConfig(newConfig);
                                    if (provider !== 'localWhisper' && provider !== 'parakeet' && provider !== 'remoteWhisper') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder="Select provider" />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">⚡ Parakeet (Recommended - Real-time / Accurate)</SelectItem>
                                    <SelectItem value="localWhisper">
                                        <div className="flex items-center gap-2">
                                            <Monitor className="h-4 w-4" />
                                            <span>Local Whisper (On Device)</span>
                                        </div>
                                    </SelectItem>
                                    <SelectItem value="remoteWhisper">
                                        <div className="flex items-center gap-2">
                                            <Globe className="h-4 w-4" />
                                            <span>Remote Whisper (Server)</span>
                                        </div>
                                    </SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {transcriptModelConfig.provider !== 'localWhisper' && transcriptModelConfig.provider !== 'parakeet' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                        <SelectValue placeholder="Select model" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {modelOptions[transcriptModelConfig.provider].map((model) => (
                                            <SelectItem key={model} value={model}>{model}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}

                        </div>
                    </div>

                    {transcriptModelConfig.provider === 'localWhisper' && (
                        <div className="mt-6">
                            <ModelManager
                                selectedModel={selectedWhisperModel}
                                onModelSelect={handleWhisperModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {transcriptModelConfig.provider === 'remoteWhisper' && (
                        <div className="mt-6 space-y-4">
                            <div className="p-4 bg-blue-50 border border-blue-200 rounded-lg">
                                <div className="flex items-center gap-2 mb-2">
                                    <Globe className="h-5 w-5 text-blue-600" />
                                    <span className="font-medium text-blue-900">Remote Whisper Server</span>
                                    {isCheckingRemote ? (
                                        <span className="ml-auto text-xs text-gray-500">Checking...</span>
                                    ) : remoteStatus === 'connected' ? (
                                        <span className="ml-auto flex items-center gap-1 text-xs text-green-600">
                                            <span className="w-2 h-2 bg-green-500 rounded-full animate-pulse"></span>
                                            Connected
                                        </span>
                                    ) : remoteStatus === 'error' ? (
                                        <span className="ml-auto flex items-center gap-1 text-xs text-red-600">
                                            <span className="w-2 h-2 bg-red-500 rounded-full"></span>
                                            Not reachable
                                        </span>
                                    ) : null}
                                </div>
                                <p className="text-sm text-blue-700 mb-3">
                                    Connect to a whisper.cpp server running on a remote machine (e.g., Windows PC with GPU).
                                </p>
                                <div className="space-y-3">
                                    <div>
                                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                                            Server URL
                                        </Label>
                                        <div className="flex gap-2">
                                            <Input
                                                type="text"
                                                value={remoteWhisperUrl}
                                                onChange={(e) => {
                                                    setRemoteWhisperUrl(e.target.value);
                                                    setRemoteStatus('unknown');
                                                }}
                                                onBlur={() => {
                                                    const newConfig = {
                                                        ...transcriptModelConfig,
                                                        remoteUrl: remoteWhisperUrl
                                                    };
                                                    setTranscriptModelConfig(newConfig);
                                                    // Auto-save when URL changes
                                                    saveTranscriptConfig(newConfig);
                                                }}
                                                placeholder="http://192.168.1.100:8178"
                                                className="flex-1"
                                            />
                                            <Button
                                                type="button"
                                                variant="outline"
                                                onClick={checkRemoteWhisperStatus}
                                                disabled={isCheckingRemote}
                                            >
                                                Test
                                            </Button>
                                        </div>
                                        <p className="text-xs text-gray-500 mt-1">
                                            Example: http://100.64.0.4:8178 (Tailscale IP)
                                        </p>
                                    </div>
                                    <div>
                                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                                            Remote Model
                                        </Label>
                                        <Select
                                            value={transcriptModelConfig.model || 'large-v3'}
                                            onValueChange={(value) => {
                                                const newConfig = {
                                                    ...transcriptModelConfig,
                                                    model: value
                                                };
                                                setTranscriptModelConfig(newConfig);
                                                // Auto-save when model changes
                                                saveTranscriptConfig(newConfig);
                                            }}
                                        >
                                            <SelectTrigger>
                                                <SelectValue placeholder="Select model" />
                                            </SelectTrigger>
                                            <SelectContent>
                                                <SelectItem value="large-v3">
                                                    <div className="flex items-center gap-2">
                                                        <span>🎯</span>
                                                        <span>large-v3 (Best Quality)</span>
                                                        <span className="ml-auto text-xs bg-purple-100 text-purple-700 px-1.5 py-0.5 rounded">Remote</span>
                                                    </div>
                                                </SelectItem>
                                                <SelectItem value="large-v3-turbo">
                                                    <div className="flex items-center gap-2">
                                                        <span>⚡</span>
                                                        <span>large-v3-turbo (Fast + Quality)</span>
                                                        <span className="ml-auto text-xs bg-purple-100 text-purple-700 px-1.5 py-0.5 rounded">Remote</span>
                                                    </div>
                                                </SelectItem>
                                                <SelectItem value="large-v2">large-v2</SelectItem>
                                                <SelectItem value="medium">medium</SelectItem>
                                                <SelectItem value="small">small</SelectItem>
                                                <SelectItem value="base">base</SelectItem>
                                                <SelectItem value="tiny">tiny (Fastest)</SelectItem>
                                            </SelectContent>
                                        </Select>
                                        {remoteStatus === 'connected' ? (
                                            <p className="text-xs text-green-600 mt-1">
                                                ✅ Server ready - ensure the selected model is loaded on the remote server
                                            </p>
                                        ) : (
                                            <p className="text-xs text-amber-600 mt-1">
                                                ⚠️ Connect to the server first, then ensure your model is loaded
                                            </p>
                                        )}
                                    </div>
                                </div>
                            </div>
                        </div>
                    )}

                    {transcriptModelConfig.provider === 'parakeet' && (
                        <div className="mt-6">
                            <ParakeetModelManager
                                selectedModel={selectedParakeetModel}
                                onModelSelect={handleParakeetModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}


                    {requiresApiKey && (
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                API Key
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 ${isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                                        }`}
                                    value={apiKey || ''}
                                    onChange={(e) => setApiKey(e.target.value)}
                                    disabled={isApiKeyLocked}
                                    onClick={handleInputClick}
                                    placeholder="Enter your API key"
                                />
                                {isApiKeyLocked && (
                                    <div
                                        onClick={handleInputClick}
                                        className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                                            }`}
                                        title={isApiKeyLocked ? "Unlock to edit" : "Lock to prevent editing"}
                                    >
                                        {isApiKeyLocked ? <Lock className="h-4 w-4" /> : <Unlock className="h-4 w-4" />}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setShowApiKey(!showApiKey)}
                                    >
                                        {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                    </Button>
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div>
    )
}








