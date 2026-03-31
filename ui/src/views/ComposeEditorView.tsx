import { useState } from 'react';
import Editor from '@monaco-editor/react';
import yaml from 'js-yaml';
import { Upload } from 'lucide-react';
import { Button } from '../components/ui';

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

const defaultYaml = `services:
  web:
    image: nginx:alpine
    ports:
      - "8080:80"
    vm:
      vcpus: 2
      memory: 1024
  api:
    image: node:20-alpine
    environment:
      - NODE_ENV=production
    vm:
      vcpus: 1
      memory: 512
`;

export function ComposeEditorView() {
  const [yamlContent, setYamlContent] = useState(defaultYaml);
  const [errors, setErrors] = useState<string[]>([]);
  const [deployStatus, setDeployStatus] = useState('');

  const handleValidation = (value: string | undefined) => {
    if (!value) return;
    try {
      yaml.load(value);
      setErrors([]);
    } catch (e: any) {
      setErrors([e.message]);
    }
  };

  const handleDeploy = async () => {
    if (errors.length > 0) return;
    setDeployStatus('Deploying...');
    try {
      await fetch(`${API_BASE}/up`, {
        method: 'POST',
        body: yamlContent,
        headers: { 'Content-Type': 'application/x-yaml' },
      });
      setDeployStatus('Deployed!');
    } catch {
      setDeployStatus('Deploy failed');
    }
    setTimeout(() => setDeployStatus(''), 3000);
  };

  return (
    <div className="p-8 max-w-6xl mx-auto h-[calc(100vh-4rem)] flex flex-col">
      <header className="mb-4 flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-white mb-1">Compose Editor</h2>
          <p className="text-sm text-slate-400">Monaco editor with YAML validation. Click Deploy to run.</p>
        </div>
        <Button onClick={handleDeploy} disabled={errors.length > 0}>
          <Upload size={16} /> Deploy
        </Button>
      </header>

      {deployStatus && (
        <div className="mb-4 p-3 rounded-lg bg-green-900/30 border border-green-700 text-green-400 text-sm">
          {deployStatus}
        </div>
      )}

      {errors.length > 0 && (
        <div className="mb-4 p-3 rounded-lg bg-red-900/30 border border-red-700 text-red-400 text-sm">
          {errors[0]}
        </div>
      )}

      <div className="flex-1 rounded-xl border border-slate-800 overflow-hidden">
        <Editor
          height="100%"
          defaultLanguage="yaml"
          value={yamlContent}
          onChange={handleValidation}
          theme="vs-dark"
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            padding: { top: 16 },
            scrollBeyondLastLine: false,
          }}
        />
      </div>
    </div>
  );
}
