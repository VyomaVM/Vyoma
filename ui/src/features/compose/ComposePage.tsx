import { useState } from 'react';
import Editor from '@monaco-editor/react';
import yaml from 'js-yaml';
import { Upload } from 'lucide-react';
import { Button } from '../../components/ui';
import { useMutation } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api-client';

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

export function ComposePage() {
  const [yamlContent, setYamlContent] = useState(defaultYaml);
  const [errors, setErrors] = useState<string[]>([]);
  const [deployStatus, setDeployStatus] = useState('');

  const deployMutation = useMutation({
    mutationFn: (content: string) => 
      apiFetch('/up', { 
        method: 'POST', 
        body: content, 
        headers: { 'Content-Type': 'application/x-yaml' } 
      }),
    onSuccess: () => {
      setDeployStatus('Deployed!');
      setTimeout(() => setDeployStatus(''), 3000);
    },
    onError: () => {
      setDeployStatus('Deploy failed');
      setTimeout(() => setDeployStatus(''), 3000);
    }
  });

  const handleValidation = (value: string | undefined) => {
    if (value !== undefined) setYamlContent(value);
    if (!value) return;
    try {
      yaml.load(value);
      setErrors([]);
    } catch (e: any) {
      setErrors([e.message]);
    }
  };

  const handleDeploy = () => {
    if (errors.length > 0) return;
    setDeployStatus('Deploying...');
    deployMutation.mutate(yamlContent);
  };

  return (
    <div className="p-8 max-w-6xl mx-auto h-[calc(100vh-4rem)] flex flex-col">
      <header className="mb-4 flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-foreground mb-1">Compose Editor</h2>
          <p className="text-sm text-muted-foreground">Monaco editor with YAML validation. Click Deploy to run.</p>
        </div>
        <Button onClick={handleDeploy} disabled={errors.length > 0 || deployMutation.isPending}>
          <Upload size={16} className="mr-2" /> {deployMutation.isPending ? 'Deploying...' : 'Deploy'}
        </Button>
      </header>

      {deployStatus && (
        <div className="mb-4 p-3 rounded-lg bg-green-500/10 border border-green-500/20 text-green-400 text-sm">
          {deployStatus}
        </div>
      )}

      {errors.length > 0 && (
        <div className="mb-4 p-3 rounded-lg bg-destructive/10 border border-destructive/20 text-destructive text-sm">
          {errors[0]}
        </div>
      )}

      <div className="flex-1 rounded-xl border border-border overflow-hidden">
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
