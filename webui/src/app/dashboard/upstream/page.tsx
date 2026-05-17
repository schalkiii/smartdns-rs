'use client';

import Box from '@mui/material/Box';
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
import DnsIcon from '@mui/icons-material/Dns';
import HearingIcon from '@mui/icons-material/Hearing';
import { useNameservers, useListeners } from '@/lib/api';

function extractProtocol(url: string): string {
  try {
    const u = new URL(url);
    return u.protocol.replace(':', '');
  } catch {
    const match = url.match(/^(\w+):\/\//);
    return match ? match[1] : 'unknown';
  }
}

export default function UpstreamPage() {
  const {
    data: nameservers,
    isLoading: nsLoading,
    error: nsError,
  } = useNameservers();

  const {
    data: listeners,
    isLoading: lsLoading,
    error: lsError,
  } = useListeners();

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 3, fontWeight: 700 }}>
        上游服务器
      </Typography>

      <Grid container spacing={3} sx={{ mb: 4 }}>
        <Grid size={{ xs: 12, sm: 6 }}>
          <Card sx={{ height: '100%' }}>
            <CardContent>
              <Box sx={{ display: 'flex', alignItems: 'center', mb: 1 }}>
                <DnsIcon sx={{ mr: 1, color: 'primary.main' }} />
                <Typography variant="body2" color="text.secondary">
                  上游服务器数量
                </Typography>
              </Box>
              {nsLoading ? (
                <Skeleton variant="text" width={60} height={48} />
              ) : (
                <Typography variant="h4" sx={{ fontWeight: 700 }}>
                  {nameservers?.count ?? 0}
                </Typography>
              )}
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6 }}>
          <Card sx={{ height: '100%' }}>
            <CardContent>
              <Box sx={{ display: 'flex', alignItems: 'center', mb: 1 }}>
                <HearingIcon sx={{ mr: 1, color: 'success.main' }} />
                <Typography variant="body2" color="text.secondary">
                  监听端口数量
                </Typography>
              </Box>
              {lsLoading ? (
                <Skeleton variant="text" width={60} height={48} />
              ) : (
                <Typography variant="h4" sx={{ fontWeight: 700 }}>
                  {listeners?.count ?? 0}
                </Typography>
              )}
            </CardContent>
          </Card>
        </Grid>
      </Grid>

      <Typography variant="h5" sx={{ mb: 2, fontWeight: 600 }}>
        上游 DNS 服务器
      </Typography>

      {nsLoading ? (
        <Box>
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} variant="rounded" height={48} sx={{ mb: 1 }} />
          ))}
        </Box>
      ) : nsError ? (
        <Alert severity="error">加载上游服务器数据失败</Alert>
      ) : !nameservers?.data?.length ? (
        <Card>
          <CardContent>
            <Typography color="text.secondary" sx={{ textAlign: 'center' }}>
              暂无上游服务器配置
            </Typography>
          </CardContent>
        </Card>
      ) : (
        <TableContainer component={Paper} variant="outlined">
          <Table>
            <TableHead>
              <TableRow>
                <TableCell>分组</TableCell>
                <TableCell>地址</TableCell>
                <TableCell>协议</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {nameservers.data.map((ns, idx) => (
                <TableRow key={idx} hover>
                  <TableCell>
                    <Typography variant="body2" sx={{ fontWeight: 500 }}>
                      {ns.group || '-'}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
                      {ns.url}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    <Chip
                      label={extractProtocol(ns.url)}
                      size="small"
                      color="primary"
                      variant="outlined"
                    />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      )}

      <Typography variant="h5" sx={{ mb: 2, mt: 4, fontWeight: 600 }}>
        监听端口
      </Typography>

      {lsLoading ? (
        <Box>
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} variant="rounded" height={48} sx={{ mb: 1 }} />
          ))}
        </Box>
      ) : lsError ? (
        <Alert severity="error">加载监听端口数据失败</Alert>
      ) : !listeners?.data?.length ? (
        <Card>
          <CardContent>
            <Typography color="text.secondary" sx={{ textAlign: 'center' }}>
              暂无监听端口配置
            </Typography>
          </CardContent>
        </Card>
      ) : (
        <TableContainer component={Paper} variant="outlined">
          <Table>
            <TableHead>
              <TableRow>
                <TableCell>地址</TableCell>
                <TableCell>端口</TableCell>
                <TableCell>协议</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {listeners.data.map((ls, idx) => (
                <TableRow key={idx} hover>
                  <TableCell>
                    <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
                      {ls.addr}
                    </Typography>
                  </TableCell>
                  <TableCell>{ls.port}</TableCell>
                  <TableCell>
                    <Chip
                      label={ls.protocol}
                      size="small"
                      color="success"
                      variant="outlined"
                    />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      )}
    </Box>
  );
}