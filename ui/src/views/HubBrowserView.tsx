import { useState } from 'react';
import { Search, Plus } from 'lucide-react';
import { Button } from '../components/ui';

interface HubImage {
  name: string;
  description: string;
  stars: number;
  pulls: string;
}

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

export function HubBrowserView() {
  const [query, setQuery] = useState('');
  const [images, setImages] = useState<HubImage[]>([]);
  const [loading, setLoading] = useState(false);

  const searchHub = async () => {
    if (!query.trim()) return;
    setLoading(true);
    try {
      const res = await fetch(`${API_BASE}/hub/search?q=${encodeURIComponent(query)}`);
      const data = await res.json();
      setImages(data.images || []);
    } catch {
      setImages([]);
    }
    setLoading(false);
  };

  const pullImage = async (name: string) => {
    await fetch(`${API_BASE}/pull`, {
      method: 'POST',
      body: JSON.stringify({ image: name }),
    });
    alert(`Pulling ${name}...`);
  };

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="mb-8">
        <h2 className="text-2xl font-bold text-white mb-1">Hub Browser</h2>
        <p className="text-sm text-slate-400">Search images from Vyoma Hub or Docker Hub.</p>
      </header>

      <div className="flex gap-3 mb-6">
        <div className="flex-1 relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" size={18} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && searchHub()}
            placeholder="Search images..."
            className="w-full bg-slate-900 border border-slate-700 rounded-lg pl-10 pr-4 py-2.5 text-white placeholder-slate-500 focus:border-orange-500 focus:outline-none"
          />
        </div>
        <Button onClick={searchHub}>Search</Button>
      </div>

      {loading ? (
        <div className="text-center py-12 text-slate-500 animate-pulse">Searching Hub...</div>
      ) : images.length === 0 ? (
        <div className="text-center py-12 text-slate-500">Search for images to browse the Hub.</div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {images.map((img, i) => (
            <div
              key={i}
              className="bg-slate-900 border border-slate-800 rounded-xl p-4 hover:border-orange-500/30 transition"
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <h3 className="font-semibold text-white">{img.name}</h3>
                  <p className="text-sm text-slate-500 mt-1 line-clamp-2">{img.description || 'No description'}</p>
                </div>
                <button
                  onClick={() => pullImage(img.name)}
                  className="p-2 bg-slate-800 hover:bg-orange-600 rounded-lg text-slate-400 hover:text-white transition ml-3"
                >
                  <Plus size={16} />
                </button>
              </div>
              <div className="flex gap-4 mt-3 text-xs text-slate-500">
                <span>★ {img.stars}</span>
                <span>↓ {img.pulls}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}