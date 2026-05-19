import { createBrowserRouter } from 'react-router-dom';
import { AppLayout } from './layout';

import { VmsPage } from '../features/vms/VmsPage';
import { ImagesPage } from '../features/images/ImagesPage';
import { VolumesPage } from '../features/volumes/VolumesPage';
import { NetworksPage } from '../features/networks/NetworksPage';
import { TimeMachinePage } from '../features/timemachine/TimeMachinePage';
import { TopologyPage } from '../features/topology/TopologyPage';
import { ComposePage } from '../features/compose/ComposePage';
import { HubPage } from '../features/hub/HubPage';
import { ClusterPage } from '../features/cluster/ClusterPage';
import { SettingsPage } from '../features/settings/SettingsPage';

// Placeholders for views that don't exist yet
const Placeholder = ({ title }: { title: string }) => (
  <div className="p-8 text-foreground">
    <h1 className="text-2xl font-bold">{title}</h1>
    <p className="mt-4 text-muted-foreground">This page is under construction.</p>
  </div>
);

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppLayout />,
    children: [
      {
        index: true,
        element: <Placeholder title="Dashboard" />,
      },
      {
        path: 'vms',
        element: <VmsPage />,
      },
      {
        path: 'images',
        element: <ImagesPage />,
      },
      {
        path: 'volumes',
        element: <VolumesPage />,
      },
      {
        path: 'networks',
        element: <NetworksPage />,
      },
      {
        path: 'timemachine',
        element: <TimeMachinePage />,
      },
      {
        path: 'topology',
        element: <TopologyPage />,
      },
      {
        path: 'compose',
        element: <ComposePage />,
      },
      {
        path: 'hub',
        element: <HubPage />,
      },
      {
        path: 'cluster',
        element: <ClusterPage />,
      },
      {
        path: 'builds',
        element: <Placeholder title="Builds" />,
      },
      {
        path: 'events',
        element: <Placeholder title="Events" />,
      },
      {
        path: 'attestation',
        element: <Placeholder title="Attestation" />,
      },
      {
        path: 'logs/:vmId',
        element: <Placeholder title="VM Logs" />,
      },
      {
        path: 'settings',
        element: <SettingsPage />,
      },
    ],
  },
]);
