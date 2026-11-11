'use client';

import { createContext, useContext } from 'react';

export type DashboardTab =
  | 'overview'
  | 'upstream'
  | 'cache'
  | 'rules'
  | 'adblock'
  | 'querylog'
  | 'settings';

export interface DashboardTabContextType {
  currentTab: DashboardTab;
  setCurrentTab: (tab: DashboardTab) => void;
}

export const DashboardTabContext = createContext<DashboardTabContextType>({
  currentTab: 'overview',
  setCurrentTab: () => {},
});

export function useDashboardTab() {
  return useContext(DashboardTabContext);
}
