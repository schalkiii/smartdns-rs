'use client';

import dynamic from 'next/dynamic';
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
import AccessTimeIcon from '@mui/icons-material/AccessTime';
import QueryStatsIcon from '@mui/icons-material/QueryStats';
import StorageIcon from '@mui/icons-material/Storage';
import BarChartIcon from '@mui/icons-material/BarChart';
import SpeedIcon from '@mui/icons-material/Speed';
import CachedIcon from '@mui/icons-material/Cached';
import { useStats, useCaches } from '@/lib/api';
import { formatUptime, formatTimestamp } from '@/lib/utils';
import { ApexOptions } from 'apexcharts';

const Chart = dynamic(() => import('react-apexcharts'), { ssr: false });

function MetricCard({
  title,
  value,
  subtitle,
  icon,
  color,
}: {
  title: string;
  value: string;
  subtitle?: string;
  icon: React.ReactNode;
  color: string;
}) {
  return (
    <Card sx={{ height: '100%' }}>
      <CardContent>
        <Box sx={{ display: 'flex', alignItems: 'center', mb: 1 }}>
          <Box
            sx={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              width: 40,
              height: 40,
              borderRadius: 1.5,
              bgcolor: `${color}20`,
              mr: 1.5,
            }}
          >
            <Box sx={{ color }}>{icon}</Box>
          </Box>
          <Typography variant="body2" color="text.secondary">
            {title}
          </Typography>
        </Box>
        <Typography variant="h4" sx={{ fontWeight: 700 }}>
          {value}
        </Typography>
        {subtitle && (
          <Typography variant="caption" color="text.secondary" sx={{ mt: 0.5, display: 'block' }}>
            {subtitle}
          </Typography>
        )}
      </CardContent>
    </Card>
  );
}

function MetricCardSkeleton() {
  return (
    <Card sx={{ height: '100%' }}>
      <CardContent>
        <Skeleton variant="rounded" width={40} height={40} sx={{ mb: 1 }} />
        <Skeleton variant="text" width="60%" height={28} />
        <Skeleton variant="text" width="80%" height={48} />
      </CardContent>
    </Card>
  );
}

export default function DashboardOverview() {
  const { data: stats, isLoading: statsLoading, error: statsError } = useStats();
  const { data: caches, isLoading: cachesLoading, error: cachesError } = useCaches();

  const hitRateValue = stats != null ? `${stats.cache_hit_rate.toFixed(1)}%` : '—';
  const avgTimeValue = stats != null ? `${stats.avg_query_time_ms.toFixed(1)} ms` : '—';

  const chartSeries = [
    {
      name: '总查询数',
      data: (stats?.history?.length ? stats.history : [{ timestamp: 0, total_queries: stats?.total_queries ?? 0, cache_hits: stats?.cache_hits ?? 0 }])
        .map((p) => ({ x: p.timestamp * 1000, y: p.total_queries })),
    },
    {
      name: '缓存命中',
      data: (stats?.history?.length ? stats.history : [{ timestamp: 0, total_queries: stats?.total_queries ?? 0, cache_hits: stats?.cache_hits ?? 0 }])
        .map((p) => ({ x: p.timestamp * 1000, y: p.cache_hits })),
    },
  ];

  const chartOptions: ApexOptions = {
    chart: {
      type: 'area',
      background: 'transparent',
      toolbar: { show: false },
      fontFamily: '"Inter", "Roboto", "Helvetica", "Arial", sans-serif',
    },
    theme: { mode: 'dark' },
    stroke: {
      curve: 'smooth',
      width: 2,
    },
    fill: {
      type: 'gradient',
      gradient: {
        shadeIntensity: 1,
        opacityFrom: 0.3,
        opacityTo: 0.05,
      },
    },
    colors: ['#42a5f5', '#66bb6a'],
    xaxis: {
      type: 'datetime',
      labels: {
        style: { colors: '#9ca3af' },
        datetimeUTC: false,
      },
    },
    yaxis: {
      labels: { style: { colors: '#9ca3af' } },
    },
    grid: {
      borderColor: '#1f2937',
      strokeDashArray: 4,
    },
    legend: {
      labels: { colors: '#9ca3af' },
    },
    tooltip: {
      theme: 'dark',
      x: {
        format: 'HH:mm:ss',
      },
    },
    dataLabels: {
      enabled: false,
    },
  };

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 3, fontWeight: 700 }}>
        系统概览
      </Typography>

      <Grid container spacing={3} sx={{ mb: 4 }}>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          {statsLoading ? (
            <MetricCardSkeleton />
          ) : statsError ? (
            <Alert severity="error">加载失败</Alert>
          ) : (
            <MetricCard
              title="运行时间"
              value={formatUptime(stats!.uptime_secs)}
              icon={<AccessTimeIcon />}
              color="#42a5f5"
            />
          )}
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          {statsLoading ? (
            <MetricCardSkeleton />
          ) : statsError ? (
            <Alert severity="error">加载失败</Alert>
          ) : (
            <MetricCard
              title="缓存命中率"
              value={hitRateValue}
              subtitle={`${stats!.cache_query_hits.toLocaleString()} 命中 / ${stats!.total_queries.toLocaleString()} 总查询`}
              icon={<CachedIcon />}
              color="#66bb6a"
            />
          )}
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          {statsLoading ? (
            <MetricCardSkeleton />
          ) : statsError ? (
            <Alert severity="error">加载失败</Alert>
          ) : (
            <MetricCard
              title="平均查询时间"
              value={avgTimeValue}
              icon={<SpeedIcon />}
              color="#ffa726"
            />
          )}
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          {statsLoading ? (
            <MetricCardSkeleton />
          ) : statsError ? (
            <Alert severity="error">加载失败</Alert>
          ) : (
            <MetricCard
              title="总查询数"
              value={stats!.total_queries.toLocaleString()}
              icon={<BarChartIcon />}
              color="#ab47bc"
            />
          )}
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          {statsLoading ? (
            <MetricCardSkeleton />
          ) : statsError ? (
            <Alert severity="error">加载失败</Alert>
          ) : (
            <MetricCard
              title="缓存条目数"
              value={stats!.cache_size.toLocaleString()}
              icon={<StorageIcon />}
              color="#ef5350"
            />
          )}
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          {statsLoading ? (
            <MetricCardSkeleton />
          ) : statsError ? (
            <Alert severity="error">加载失败</Alert>
          ) : (
            <MetricCard
              title="活跃查询数"
              value={String(stats!.active_queries)}
              icon={<QueryStatsIcon />}
              color="#26a69a"
            />
          )}
        </Grid>
      </Grid>

      <Card sx={{ mb: 4 }}>
        <CardContent>
          <Typography variant="h6" sx={{ mb: 2, fontWeight: 600 }}>
            查询趋势
          </Typography>
          <Box sx={{ height: 300 }}>
            <Chart
              options={chartOptions}
              series={chartSeries}
              type="area"
              height="100%"
            />
          </Box>
        </CardContent>
      </Card>

      <Card>
        <CardContent>
          <Typography variant="h6" sx={{ mb: 2, fontWeight: 600 }}>
            缓存条目
          </Typography>
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
          ) : !caches?.data?.length ? (
            <Typography color="text.secondary" sx={{ py: 4, textAlign: 'center' }}>
              暂无缓存条目
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
                  {caches.data.slice(0, 10).map((entry) => (
                    <TableRow key={entry.name} hover>
                      <TableCell>
                        <Typography variant="body2" sx={{ fontFamily: 'monospace' }}>
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
                      <TableCell>{entry.hits.toLocaleString()}</TableCell>
                      <TableCell>{formatTimestamp(entry.last_access)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          )}
        </CardContent>
      </Card>
    </Box>
  );
}