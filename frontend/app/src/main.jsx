/* Phoskonomia — app entry. Imports order matters:
   (a) all CSS, (b) the scanner engine (registers window.OscScanner),
   (c) data + shared component modules for side effects (populate window.* before
       render so any missed ES import still resolves at runtime),
   (d) BrowserRouter + Routes mounted under the App layout route. */

// (a) styles
import './styles/fonts/fonts.css'
import './styles/colors_and_type.css'
import './styles/osc-crt.css'
import './styles/phosk.css'
import './styles/shell.css'
import './styles/txn.css'
import './styles/budget.css'
import './styles/subs.css'
import './styles/debt.css'
import './styles/analytics.css'
import './styles/config.css'

// (b) scanner engine — sets window.OscScanner
import './lib/osc-scanner.js'

// (c) shared layer — side-effect imports populate window.* before first render
import './data/phosk.js'
import './components/prims.jsx'
import './components/comps.jsx'
import './components/shell.jsx'
import './components/states.jsx'
import './lib/tweaks.jsx'

// (d) router
import React from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'

import App from './App.jsx'
import DashboardPage from './pages/Dashboard.jsx'
import TransactionsPage from './pages/Transactions.jsx'
import BudgetsPage from './pages/Budgets.jsx'
import SubscriptionsPage from './pages/Subscriptions.jsx'
import DebtsPage from './pages/Debts.jsx'
import AnalyticsPage from './pages/Analytics.jsx'
import ConfigPage from './pages/Config.jsx'

createRoot(document.getElementById('root')).render(
  <BrowserRouter>
    <Routes>
      <Route element={<App />}>
        <Route index element={<Navigate to="/dashboard" replace />} />
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/transactions" element={<TransactionsPage />} />
        <Route path="/budgets" element={<BudgetsPage />} />
        <Route path="/subscriptions" element={<SubscriptionsPage />} />
        <Route path="/debts" element={<DebtsPage />} />
        <Route path="/analytics" element={<AnalyticsPage />} />
        <Route path="/config" element={<ConfigPage />} />
        <Route path="*" element={<Navigate to="/dashboard" replace />} />
      </Route>
    </Routes>
  </BrowserRouter>
)
