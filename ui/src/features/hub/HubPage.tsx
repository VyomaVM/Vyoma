import { useState } from 'react';
import { Search, Download, Star } from 'lucide-react';
import { Button, Input, Card, Loading, EmptyState } from '../../components/ui';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../lib/api-client';

export function HubPage() {
  const [query, setQuery] = useState('');
  const [searchQuery, setSearchQuery] = useState('');

  const { data: results, isLoading } = useQuery({
    queryKey: ['hubSearch', searchQuery],
    queryFn: () => api.get<any[]>(`/hub/search?q=${encodeURIComponent(searchQuery)}`),
    enabled: !!searchQuery,
  });

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (query.trim()) {
      setSearchQuery(query.trim());
    }
  };

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="mb-8">
        <h2 className="text-2xl font-bold text-foreground mb-1">Image Hub</h2>
        <p className="text-sm text-muted-foreground">Search and pull images from the registry.</p>
      </header>

      <form onSubmit={handleSearch} className="flex gap-4 mb-8">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" size={18} />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search images (e.g., ubuntu, node)..."
            className="pl-10"
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          Search
        </Button>
      </form>

      <div className="space-y-4">
        {isLoading ? (
          <Loading text="Searching..." />
        ) : !searchQuery && !results ? (
          <div className="py-12 text-center text-muted-foreground">
            Enter a search term to find images.
          </div>
        ) : results?.length === 0 ? (
          <EmptyState title="No results found" description={`No images found for "${searchQuery}"`} icon={<Search size={48} />} />
        ) : (
          results?.map((img, i) => (
            <Card key={i} className="p-4 flex items-center justify-between group">
              <div>
                <h3 className="font-bold text-foreground text-lg">{img.name}</h3>
                <p className="text-sm text-muted-foreground mt-1 max-w-2xl">{img.description}</p>
                <div className="flex gap-4 mt-3 text-xs text-muted-foreground">
                  <span className="flex items-center gap-1"><Star size={14} className="text-yellow-500" /> {img.stars}</span>
                  <span className="flex items-center gap-1"><Download size={14} /> {img.pulls}</span>
                </div>
              </div>
              <Button variant="outline" className="opacity-0 group-hover:opacity-100 transition">
                <Download size={16} className="mr-2" /> Pull
              </Button>
            </Card>
          ))
        )}
      </div>
    </div>
  );
}
