import React from "react";
import { NavLink, Route, Routes } from "react-router-dom";

const Dashboard = React.lazy(() => import("./pages/Dashboard"));
const InstanceForm = React.lazy(() => import("./pages/InstanceForm"));
const TerminalPage = React.lazy(() => import("./pages/Terminal"));
const Settings = React.lazy(() => import("./pages/Settings"));

export default function App() {
  return (
    <div className="app-root">
      <header className="app-header">
        <div className="app-header-inner">
          <div className="app-brand">
            <span className="brand-mark" aria-hidden="true">
              &gt;_
            </span>
            <span className="app-title">AI CLI Manager</span>
          </div>

          <nav className="app-nav" aria-label="Primary">
            <NavLink to="/" end className={({ isActive }) => `app-nav-link${isActive ? " active" : ""}`}>
              Dashboard
            </NavLink>
            <NavLink to="/instances/new" className={({ isActive }) => `app-nav-link${isActive ? " active" : ""}`}>
              New Instance
            </NavLink>
            <NavLink to="/settings" className={({ isActive }) => `app-nav-link${isActive ? " active" : ""}`}>
              Settings
            </NavLink>
          </nav>
        </div>
      </header>

      <main className="page-wrap">
        <React.Suspense
          fallback={
            <section className="surface-card card-content">
              <p className="muted">Loading page...</p>
            </section>
          }
        >
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/instances/new" element={<InstanceForm mode="create" />} />
            <Route path="/instances/:id/edit" element={<InstanceForm mode="edit" />} />
            <Route path="/instances/:id/term" element={<TerminalPage />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </React.Suspense>
      </main>
    </div>
  );
}
