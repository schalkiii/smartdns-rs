'use client';

import { useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import IconButton from '@mui/material/IconButton';
import Skeleton from '@mui/material/Skeleton';
import Tab from '@mui/material/Tab';
import Tabs from '@mui/material/Tabs';
import Table from '@mui/material/Table';
import TableBody from '@mui/material/TableBody';
import TableCell from '@mui/material/TableCell';
import TableContainer from '@mui/material/TableContainer';
import TableHead from '@mui/material/TableHead';
import TableRow from '@mui/material/TableRow';
import Paper from '@mui/material/Paper';
import Alert from '@mui/material/Alert';
import Chip from '@mui/material/Chip';
import TextField from '@mui/material/TextField';
import Typography from '@mui/material/Typography';
import Snackbar from '@mui/material/Snackbar';
import CircularProgress from '@mui/material/CircularProgress';
import AddIcon from '@mui/icons-material/Add';
import DeleteIcon from '@mui/icons-material/Delete';
import {
  useAddresses,
  useCreateAddress,
  useDeleteAddress,
  useForwards,
} from '@/lib/api';

interface TabPanelProps {
  children: React.ReactNode;
  value: number;
  index: number;
}

function TabPanel({ children, value, index }: TabPanelProps) {
  return (
    <Box role="tabpanel" hidden={value !== index} sx={{ pt: 3 }}>
      {value === index && children}
    </Box>
  );
}

function AddressRulesTab() {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [domain, setDomain] = useState('');
  const [ip, setIp] = useState('');
  const [snackbar, setSnackbar] = useState<{
    open: boolean;
    message: string;
    severity: 'success' | 'error';
  }>({ open: false, message: '', severity: 'success' });

  const {
    data: addresses,
    isLoading,
    error,
  } = useAddresses();

  const createAddress = useCreateAddress();
  const deleteAddress = useDeleteAddress();

  const handleAdd = () => {
    if (!domain || !ip) return;
    createAddress.mutate(
      { domain, ip },
      {
        onSuccess: () => {
          setSnackbar({
            open: true,
            message: '地址规则添加成功',
            severity: 'success',
          });
          setDialogOpen(false);
          setDomain('');
          setIp('');
        },
        onError: () => {
          setSnackbar({
            open: true,
            message: '添加地址规则失败',
            severity: 'error',
          });
        },
      }
    );
  };

  const handleDelete = (d: string) => {
    deleteAddress.mutate(d, {
      onSuccess: () => {
        setSnackbar({
          open: true,
          message: '地址规则已删除',
          severity: 'success',
        });
      },
      onError: () => {
        setSnackbar({
          open: true,
          message: '删除地址规则失败',
          severity: 'error',
        });
      },
    });
  };

  return (
    <Box>
      <Box
        sx={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          mb: 2,
        }}
      >
        <Typography variant="h6" sx={{ fontWeight: 600 }}>
          地址规则 ({addresses?.count ?? 0})
        </Typography>
        <Button
          variant="contained"
          startIcon={<AddIcon />}
          onClick={() => setDialogOpen(true)}
        >
          添加规则
        </Button>
      </Box>

      {isLoading ? (
        <Box>
          {Array.from({ length: 5 }).map((_, i) => (
            <Skeleton key={i} variant="rounded" height={48} sx={{ mb: 1 }} />
          ))}
        </Box>
      ) : error ? (
        <Alert severity="error">加载地址规则失败</Alert>
      ) : !addresses?.data?.length ? (
        <Card variant="outlined">
          <CardContent>
            <Typography color="text.secondary" sx={{ textAlign: 'center' }}>
              暂无地址规则
            </Typography>
          </CardContent>
        </Card>
      ) : (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>域名</TableCell>
                <TableCell>IP 地址</TableCell>
                <TableCell align="right">操作</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {addresses.data.map((entry) => (
                <TableRow key={entry.domain} hover>
                  <TableCell>
                    <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
                      {entry.domain}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    <Chip
                      label={entry.ip}
                      size="small"
                      variant="outlined"
                      color="primary"
                    />
                  </TableCell>
                  <TableCell align="right">
                    <IconButton
                      size="small"
                      color="error"
                      onClick={() => handleDelete(entry.domain)}
                      disabled={deleteAddress.isPending}
                    >
                      <DeleteIcon fontSize="small" />
                    </IconButton>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      )}

      <Dialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        maxWidth="sm"
        fullWidth
      >
        <DialogTitle>添加地址规则</DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            margin="dense"
            label="域名"
            fullWidth
            value={domain}
            onChange={(e) => setDomain(e.target.value)}
            placeholder="example.com"
            sx={{ mb: 2 }}
          />
          <TextField
            margin="dense"
            label="IP 地址"
            fullWidth
            value={ip}
            onChange={(e) => setIp(e.target.value)}
            placeholder="1.2.3.4"
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDialogOpen(false)}>取消</Button>
          <Button
            variant="contained"
            onClick={handleAdd}
            disabled={createAddress.isPending || !domain || !ip}
            startIcon={
              createAddress.isPending ? (
                <CircularProgress size={16} color="inherit" />
              ) : undefined
            }
          >
            添加
          </Button>
        </DialogActions>
      </Dialog>

      <Snackbar
        open={snackbar.open}
        autoHideDuration={3000}
        onClose={() => setSnackbar((s) => ({ ...s, open: false }))}
      >
        <Alert
          onClose={() => setSnackbar((s) => ({ ...s, open: false }))}
          severity={snackbar.severity}
          variant="filled"
        >
          {snackbar.message}
        </Alert>
      </Snackbar>
    </Box>
  );
}

function ForwardRulesTab() {
  const { data: forwards, isLoading, error } = useForwards();

  return (
    <Box>
      <Box
        sx={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          mb: 2,
        }}
      >
        <Typography variant="h6" sx={{ fontWeight: 600 }}>
          转发规则 ({forwards?.count ?? 0})
        </Typography>
      </Box>

      {isLoading ? (
        <Box>
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} variant="rounded" height={48} sx={{ mb: 1 }} />
          ))}
        </Box>
      ) : error ? (
        <Alert severity="error">加载转发规则失败</Alert>
      ) : !forwards?.data?.length ? (
        <Card variant="outlined">
          <CardContent>
            <Typography color="text.secondary" sx={{ textAlign: 'center' }}>
              暂无转发规则
            </Typography>
          </CardContent>
        </Card>
      ) : (
        forwards.data.map((group) => (
          <Card key={group.name} sx={{ mb: 2 }} variant="outlined">
            <CardContent>
              <Box
                sx={{
                  display: 'flex',
                  alignItems: 'center',
                  mb: 1.5,
                }}
              >
                <Typography variant="subtitle1" sx={{ fontWeight: 600, mr: 1 }}>
                  {group.name}
                </Typography>
                <Chip
                  label={`${group.count} 条记录`}
                  size="small"
                  color="primary"
                  variant="outlined"
                />
              </Box>
              <TableContainer component={Paper} variant="outlined">
                <Table size="small">
                  <TableHead>
                    <TableRow>
                      <TableCell>转发目标</TableCell>
                    </TableRow>
                  </TableHead>
                  <TableBody>
                    {Array.isArray(group.forwards) &&
                      group.forwards.map((fw, idx) => (
                        <TableRow key={idx} hover>
                          <TableCell>
                            <Typography
                              variant="body2"
                              sx={{ fontFamily: 'monospace' }}
                            >
                              {typeof fw === 'string'
                                ? fw
                                : JSON.stringify(fw)}
                            </Typography>
                          </TableCell>
                        </TableRow>
                      ))}
                  </TableBody>
                </Table>
              </TableContainer>
            </CardContent>
          </Card>
        ))
      )}
    </Box>
  );
}

export default function RulesPage() {
  const [tab, setTab] = useState(0);

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 3, fontWeight: 700 }}>
        规则管理
      </Typography>

      <Card>
        <CardContent>
          <Tabs
            value={tab}
            onChange={(_, v) => setTab(v)}
            sx={{ borderBottom: 1, borderColor: 'divider' }}
          >
            <Tab label="地址规则" />
            <Tab label="转发规则" />
          </Tabs>

          <TabPanel value={tab} index={0}>
            <AddressRulesTab />
          </TabPanel>
          <TabPanel value={tab} index={1}>
            <ForwardRulesTab />
          </TabPanel>
        </CardContent>
      </Card>
    </Box>
  );
}