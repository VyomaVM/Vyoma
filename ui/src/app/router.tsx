import { createBrowserRouter } from 'react-router-dom';
import { AppLayout } from './layout';

import { DashboardPage } from '../features/dashboard/DashboardPage';
import { VmsPage } from '../features/vms/VmsPage';
import { ImagesPage } from '../features/images/ImagesPage';
import { VolumesPage } from '../features/volumes/VolumesPage';
import { NetworksPage } from '../features/networks/NetworksPage';
import { TimeMachinePage } from '../features/timemachine/TimeMachinePage';
import { TopologyPage } from '../features/topology/TopologyPage';
import { ComposePage } from '../features/compose/ComposePage';
import { HubPage } from '../features/hub/HubPage';
import { ClusterPage } from '../features/cluster/ClusterPage';
import { BuildsPage } from '../features/builds/BuildsPage';
import { EventsPage } from '../features/events/EventsPage';
import { AttestationPage } from '../features/attestation/AttestationPage';
import { LogViewerPage } from '../features/logs/LogViewerPage';
import { SettingsPage } from '../features/settings/SettingsPage';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppLayout />,
    children: [
      {
        index: true,
        element: <DashboardPage />,
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
        element: <BuildsPage />,
      },
      {
        path: 'events',
        element: <EventsPage />,
      },
      {
        path: 'attestation',
        element: <AttestationPage />,
      },
      {
        path: 'logs/:vmId',
        element: <LogViewerPage />,
      },
      {
        path: 'settings',
        element: <SettingsPage />,
      },
    ],
  },
]);
