'use client';

import { useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import Grid from '@mui/material/Grid';
import Skeleton from '@mui/material/Skeleton';
import Typography from '@mui/material/Typography';
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
import InputAdornment from '@mui/material/InputAdornment';
import Snackbar from '@mui/material/Snackbar';
import CircularProgress from '@mui/material/CircularProgress';
import DeleteSweepIcon from '@mui/icons-material/DeleteSweep';
import SearchIcon from '@mui/icons-material/Search';
import StorageIcon from '@mui/icons-material/Storage';
import { useCaches, useFlushCache, useCacheConfig } from '@/lib/api';
import { formatTimestamp } from '@/lib/utils';

export default function CachePage() {
  const [search, setSearch] = useState('');
  const [snackbar, setSnackbar] = useState<{
    open: boolean;
    message: string;
    severity: 'success' | 'error';
  }>({ open: false, message: '', severity: 'success' });

  const {
    data: caches,
    isLoading: cachesLoading,
    error: cachesError,
  } = useCaches();

  const {
    data: cacheConfig,
    isLoading: configLoading,
  } = useCacheConfig();

  const flushCache = useFlushCache();

  const handleFlush = () => {
    flushCache.mutate(undefined, {
      onSuccess: () => {
        setSnackbar({
          open: true,
          message: '缓存已清空',
          severity: 'success',
        });
      },
      onError: () => {
        setSnackbar({
          open: true,
          message: '清空缓存失败',
          severity: 'error',
        });
      },
    });
  };

  const filteredCaches = caches?.data?.filter((entry) =>
    entry.name.toLowerCase().includes(search.toLowerCase())
  ) ?? [];

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 3, fontWeight: 700 }}>
        缓存管理
      </Typography>

      <Grid container spacing={3} sx={{ mb: 4 }}>
        <Grid size={{ xs: 12, sm: 6, lg: 3 }}>
          <Card sx={{ height: '100%' }}>
            <CardContent>
              <Box sx={{ display: 'flex', alignItems: 'center', mb: 1 }}>
                <StorageIcon sx={{ mr: 1, color: 'primary.main' }} />
                <Typography variant="body2" color="text.secondary">
                  缓存大小限制
                </Typography>
              </Box>
              {configLoading ? (
                <Skeleton variant="text" width={80} height={48} />
              ) : (
                <Typography variant="h4" sx={{ fontWeight: 700 }}>
                  {cacheConfig?.size ?? '-'}
                </Typography>
              )}
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 3 }}>
          <Card sx={{ height: '100%' }}>
            <CardContent>
              <Box sx={{ display: 'flex', alignItems: 'center', mb: 1 }}>
                <StorageIcon sx={{ mr: 1, color: 'warning.main' }} />
                <Typography variant="body2" color="text.secondary">
                  当前缓存条目
                </Typography>
              </Box>
              {cachesLoading ? (
                <Skeleton variant="text" width={80} height={48} />
              ) : (
                <Typography variant="h4" sx={{ fontWeight: 700 }}>
                  {caches?.count ?? 0}
                </Typography>
              )}
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 3 }}>
          <Card sx={{ height: '100%', display: 'flex', alignItems: 'center' }}>
            <CardContent sx={{ width: '100%' }}>
              <Button
                variant="contained"
                color="error"
                startIcon={
                  flushCache.isPending ? (
                    <CircularProgress size={18} color="inherit" />
                  ) : (
                    <DeleteSweepIcon />
                  )
                }
                onClick={handleFlush}
                disabled={flushCache.isPending}
                fullWidth
                sx={{ py: 1.5 }}
              >
                清空缓存
              </Button>
            </CardContent>
          </Card>
        </Grid>
      </Grid>

      <Card>
        <CardContent>
          <Box
            sx={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              mb: 2,
              flexWrap: 'wrap',
              gap: 2,
            }}
          >
            <Typography variant="h6" sx={{ fontWeight: 600 }}>
              缓存条目
            </Typography>
            <TextField
              size="small"
              placeholder="搜索域名..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              slotProps={{
                input: {
                  startAdornment: (
                    <InputAdornment position="start">
                      <SearchIcon fontSize="small" />
                    </InputAdornment>
                  ),
                },
              }}
              sx={{ minWidth: 260 }}
            />
          </Box>

          {cachesLoading ? (
            <Box>
              {Array.from({ length: 5 }).map((_, i) => (
                <Skeleton
                  key={i}
                  variant="rounded"
                  height={48}
                  sx={{ mb: 1 }}
                />
              ))}
            </Box>
          ) : cachesError ? (
            <Alert severity="error">加载缓存数据失败</Alert>
          ) : filteredCaches.length === 0 ? (
            <Typography
              color="text.secondary"
              sx={{ py: 4, textAlign: 'center' }}
            >
              {search ? '未找到匹配的缓存条目' : '暂无缓存条目'}
            </Typography>
          ) : (
            <TableContainer component={Paper} variant="outlined">
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell>域名</TableCell>
                    <TableCell>类型</TableCell>
                    <TableCell>命中次数</TableCell>
                    <TableCell>最后访问</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {filteredCaches.map((entry) => (
                    <TableRow key={entry.name} hover>
                      <TableCell>
                        <Typography
                          variant="body2"
                          sx={{ fontFamily: 'monospace' }}
                        >
                          {entry.name}
                        </Typography>
                      </TableCell>
                      <TableCell>
                        <Chip
                          label={entry.query_type}
                          size="small"
                          variant="outlined"
                        />
                      </TableCell>
                      <TableCell>
                        {entry.hits.toLocaleString()}
                      </TableCell>
                      <TableCell>
                        {formatTimestamp(entry.last_access)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          )}
        </CardContent>
      </Card>

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