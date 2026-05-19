import { createBrowserRouter } from 'react-router-dom';
import { AppLayout } from './layout';

// Placeholders for views that don't exist yet
const Placeholder = ({ title }: { title: string }) => (
  <div className="p-8 text-foreground">
    <h1 className="text-2xl font-bold">{title}</h1>
    <p className="mt-4 text-muted-foreground">This page is under construction.</p>
  </div>
);

// We import existing views if they are meant to be simple placeholders for now.
import {
  MicroVMsView,
  TimeMachineView,
  TopologyView,
  ComposeEditorView,
  HubBrowserView,
  ImagesView,
  VolumesView,
  NetworksView,
  SettingsView
} from '../views';

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
        element: <MicroVMsView />,
      },
      {
        path: 'images',
        element: <ImagesView />,
      },
      {
        path: 'volumes',
        element: <VolumesView />,
      },
      {
        path: 'networks',
        element: <NetworksView />,
      },
      {
        path: 'timemachine',
        element: <TimeMachineView />,
      },
      {
        path: 'topology',
        element: <TopologyView />,
      },
      {
        path: 'compose',
        element: <ComposeEditorView />,
      },
      {
        path: 'hub',
        element: <HubBrowserView />,
      },
      {
        path: 'cluster',
        element: <Placeholder title="Cluster" />,
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
        element: <SettingsView />,
      },
    ],
  },
]);
