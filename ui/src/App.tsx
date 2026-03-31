import { useState } from 'react';
import { Layout } from './components/Layout';
import {
  MicroVMsView,
  ImagesView,
  VolumesView,
  NetworksView,
  TimeMachineView,
  TopologyView,
  ComposeEditorView,
  HubBrowserView,
  StatsView,
  SettingsView,
} from './views';

function App() {
  const [activeTab, setActiveTab] = useState('vms');

  const renderView = () => {
    switch (activeTab) {
      case 'vms': return <MicroVMsView />;
      case 'images': return <ImagesView />;
      case 'volumes': return <VolumesView />;
      case 'networks': return <NetworksView />;
      case 'timemachine': return <TimeMachineView />;
      case 'topology': return <TopologyView />;
      case 'compose': return <ComposeEditorView />;
      case 'hub': return <HubBrowserView />;
      case 'stats': return <StatsView />;
      case 'settings': return <SettingsView />;
      default: return <MicroVMsView />;
    }
  };

  return <Layout activeTab={activeTab} onTabChange={setActiveTab}>{renderView()}</Layout>;
}

export default App;
