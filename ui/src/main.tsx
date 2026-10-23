import { MutationCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { useQuery } from "@tanstack/react-query";
import { BrowserRouter, Navigate, Route, Routes } from "react-router";
import { Layout } from "@/components/layout";
import { Toasts, announce } from "@/components/toasts";
import { ThemeProvider } from "@/components/theme";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ContractsPage } from "@/pages/contracts";
import { ConventionsPage } from "@/pages/conventions";
import { ExplorerPage } from "@/pages/explorer";
import { FlowsPage } from "@/pages/flows";
import { LedgerPage } from "@/pages/ledger";
import { OverviewPage } from "@/pages/overview";
import { PackagesPage } from "@/pages/packages";
import { RepositoriesPage } from "@/pages/repositories";
import { SettingsPage, SettingsShell } from "@/pages/settings";
import { SetupPage } from "@/pages/setup";
import { AccessPage, AuditPage, IntegrationsPage, MembersPage } from "@/pages/access";
import { SignInPage } from "@/pages/signin";
import { DevicePage } from "@/pages/device";
import { ApiError, api } from "@/lib/api";
import "./index.css";

const queries = new QueryClient({
  // A refusal (not signed in, not allowed) is final; only other failures get one retry.
  defaultOptions: {
    queries: { staleTime: 60_000, retry: (n, e) => n < 1 && !(e instanceof ApiError && (e.status === 401 || e.status === 403)) },
  },
  mutationCache: new MutationCache({
    onSuccess: (data, _v, _c, m) => announce(data, null, m.meta),
    onError: (e, _v, _c, m) => announce(null, e, m.meta),
  }),
});

/** On a server, nothing but the sign-in page until someone is signed in. */
function Gate({ children }: { children: React.ReactNode }) {
  const me = useQuery({
    queryKey: ["me"],
    queryFn: api.me,
    staleTime: Infinity,
  });
  if (me.isPending) return null;
  if (me.data?.mode === "server" && (!me.data.signed_in || window.location.pathname === "/signin")) return <SignInPage me={me.data} />;
  return <>{children}</>;
}

// Who is signed in is asked once; asking again on every page would unmount the app while it waits.
queries.setQueryDefaults(["me"], {
  staleTime: Infinity,
  retry: false,
  retryOnMount: false,
  refetchOnMount: false,
  refetchOnWindowFocus: false,
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <QueryClientProvider client={queries}>
        <TooltipProvider>
          <Toasts />
          <BrowserRouter>
            <Gate>
              <Routes>
                <Route path="device" element={<DevicePage />} />
                <Route element={<Layout />}>
                  <Route index element={<OverviewPage />} />
                  <Route path="contracts" element={<ContractsPage />} />
                  <Route path="ledger" element={<LedgerPage />} />
                  <Route path="explorer" element={<ExplorerPage />} />
                  <Route path="conventions" element={<ConventionsPage />} />
                  <Route path="packages" element={<PackagesPage />} />
                  <Route path="flows" element={<FlowsPage />} />
                  <Route path="setup" element={<SetupPage />} />
                  <Route path="settings" element={<SettingsShell />}>
                    <Route index element={<SettingsPage />} />
                    <Route path="repositories" element={<RepositoriesPage />} />
                    <Route path="members" element={<MembersPage />} />
                    <Route path="integrations" element={<IntegrationsPage />} />
                    <Route path="audit" element={<AuditPage />} />
                    <Route path="access" element={<AccessPage />} />
                  </Route>
                  <Route path="repositories" element={<Navigate to="/settings/repositories" replace />} />
                  <Route path="*" element={<OverviewPage />} />
                </Route>
              </Routes>
            </Gate>
          </BrowserRouter>
        </TooltipProvider>
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>,
);
