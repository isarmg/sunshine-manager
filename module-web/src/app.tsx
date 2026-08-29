import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SunshineView as LegacySunshineView } from "./features/sunshine/SunshineView";
import { LogsView } from "./features/logs/LogsView";
import { Fragment, createElement as h } from "./runtime";

interface ComponentProps {
  actionRequest: number;
  onActionRequestHandled: (request: number) => void;
  hasPermission: (permission: string) => boolean;
}

export function activate() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { refetchOnWindowFocus: false, retry: 1, staleTime: 5_000 },
      mutations: { retry: false },
    },
  });

  function SunshineView(props: ComponentProps) {
    return (
      <QueryClientProvider client={queryClient}>
        <LegacySunshineView
          addTrigger={props.actionRequest}
          onAddTriggerHandled={props.onActionRequestHandled}
          canWrite={props.hasPermission("sunshine.hosts.write")}
          canProxy={props.hasPermission("sunshine.proxy.use")}
        />
      </QueryClientProvider>
    );
  }

  function SunshineLogsView() {
    return (
      <QueryClientProvider client={queryClient}>
        <LogsView />
      </QueryClientProvider>
    );
  }

  return {
    components: { SunshineView, SunshineLogsView },
    primaryActions: [{
      component: "SunshineView",
      label: "添加 Sunshine 主机",
      permission: "sunshine.hosts.write",
    }],
  };
}
