'use client';

import { createContext, useContext, useState } from 'react';
import Box from '@mui/material/Box';
import Drawer from '@mui/material/Drawer';
import AppBar from '@mui/material/AppBar';
import Toolbar from '@mui/material/Toolbar';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemButton from '@mui/material/ListItemButton';
import ListItemIcon from '@mui/material/ListItemIcon';
import ListItemText from '@mui/material/ListItemText';
import Typography from '@mui/material/Typography';
import IconButton from '@mui/material/IconButton';
import Divider from '@mui/material/Divider';
import DashboardIcon from '@mui/icons-material/Dashboard';
import DnsIcon from '@mui/icons-material/Dns';
import StorageIcon from '@mui/icons-material/Storage';
import RuleIcon from '@mui/icons-material/Rule';
import MenuIcon from '@mui/icons-material/Menu';

const DRAWER_WIDTH = 240;

export type DashboardTab = 'overview' | 'upstream' | 'cache' | 'rules';

interface DashboardTabContextType {
  currentTab: DashboardTab;
  setCurrentTab: (tab: DashboardTab) => void;
}

const DashboardTabContext = createContext<DashboardTabContextType>({
  currentTab: 'overview',
  setCurrentTab: () => {},
});

export function useDashboardTab() {
  return useContext(DashboardTabContext);
}

const navItems: { label: string; icon: React.ReactNode; tab: DashboardTab }[] = [
  { label: '系统概览', icon: <DashboardIcon />, tab: 'overview' },
  { label: '上游服务器', icon: <DnsIcon />, tab: 'upstream' },
  { label: '缓存管理', icon: <StorageIcon />, tab: 'cache' },
  { label: '规则管理', icon: <RuleIcon />, tab: 'rules' },
];

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const [currentTab, setCurrentTab] = useState<DashboardTab>('overview');
  const [mobileOpen, setMobileOpen] = useState(false);

  const drawer = (
    <Box>
      <Toolbar>
        <Typography variant="h6" noWrap sx={{ fontWeight: 700 }}>
          SmartDNS
        </Typography>
      </Toolbar>
      <Divider />
      <List>
        {navItems.map((item) => (
          <ListItem key={item.tab} disablePadding>
            <ListItemButton
              onClick={() => {
                setCurrentTab(item.tab);
                setMobileOpen(false);
              }}
              selected={currentTab === item.tab}
              sx={{
                borderRadius: 0,
                '&.Mui-selected': {
                  borderRight: 3,
                  borderColor: 'primary.main',
                },
              }}
            >
              <ListItemIcon
                sx={{
                  color:
                    currentTab === item.tab ? 'primary.main' : 'inherit',
                }}
              >
                {item.icon}
              </ListItemIcon>
              <ListItemText primary={item.label} />
            </ListItemButton>
          </ListItem>
        ))}
      </List>
    </Box>
  );

  return (
    <DashboardTabContext.Provider value={{ currentTab, setCurrentTab }}>
      <Box sx={{ display: 'flex', minHeight: '100vh' }}>
        <AppBar
          position="fixed"
          sx={{ zIndex: (theme) => theme.zIndex.drawer + 1 }}
        >
          <Toolbar>
            <IconButton
              color="inherit"
              edge="start"
              onClick={() => setMobileOpen(!mobileOpen)}
              sx={{ mr: 2, display: { md: 'none' } }}
            >
              <MenuIcon />
            </IconButton>
            <Typography variant="h6" noWrap>
              SmartDNS Dashboard
            </Typography>
          </Toolbar>
        </AppBar>
        <Drawer
          variant="permanent"
          sx={{
            width: DRAWER_WIDTH,
            flexShrink: 0,
            display: { xs: 'none', md: 'block' },
            '& .MuiDrawer-paper': {
              width: DRAWER_WIDTH,
              boxSizing: 'border-box',
            },
          }}
          open
        >
          {drawer}
        </Drawer>
        <Drawer
          variant="temporary"
          open={mobileOpen}
          onClose={() => setMobileOpen(false)}
          ModalProps={{ keepMounted: true }}
          sx={{
            display: { xs: 'block', md: 'none' },
            '& .MuiDrawer-paper': {
              width: DRAWER_WIDTH,
              boxSizing: 'border-box',
            },
          }}
        >
          {drawer}
        </Drawer>
        <Box
          component="main"
          sx={{
            flexGrow: 1,
            p: 3,
            mt: 8,
            width: { md: `calc(100% - ${DRAWER_WIDTH}px)` },
            minHeight: 'calc(100vh - 64px)',
          }}
        >
          {children}
        </Box>
      </Box>
    </DashboardTabContext.Provider>
  );
}