"use client";

import dynamic from "next/dynamic";
import Box from "@mui/material/Box";
import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import Grid from "@mui/material/Grid";
import Skeleton from "@mui/material/Skeleton";
import Typography from "@mui/material/Typography";
import Table from "@mui/material/Table";
import TableBody from "@mui/material/TableBody";
import TableCell from "@mui/material/TableCell";
import TableContainer from "@mui/material/TableContainer";
import TableHead from "@mui/material/TableHead";
import TableRow from "@mui/material/TableRow";
import Paper from "@mui/material/Paper";
import Alert from "@mui/material/Alert";
import Chip from "@mui/material/Chip";
import List from "@mui/material/List";
import ListItem from "@mui/material/ListItem";
import ListItemText from "@mui/material/ListItemText";
import AccessTimeIcon from "@mui/icons-material/AccessTime";
import QueryStatsIcon from "@mui/icons-material/QueryStats";
import StorageIcon from "@mui/icons-material/Storage";
import CachedIcon from "@mui/icons-material/Cached";
import SpeedIcon from "@mui/icons-material/Speed";
import TimerIcon from "@mui/icons-material/Timer";
import CheckCircleIcon from "@mui/icons-material/CheckCircle";
import DnsIcon from "@mui/icons-material/Dns";
import HearingIcon from "@mui/icons-material/Hearing";
import DeleteSweepIcon from "@mui/icons-material/DeleteSweep";
import SearchIcon from "@mui/icons-material/Search";
import AddIcon from "@mui/icons-material/Add";
import DeleteIcon from "@mui/icons-material/Delete";
import PauseIcon from "@mui/icons-material/Pause";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import LanguageIcon from "@mui/icons-material/Language";
import PeopleIcon from "@mui/icons-material/People";
import Button from "@mui/material/Button";
import TextField from "@mui/material/TextField";
import InputAdornment from "@mui/material/InputAdornment";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import IconButton from "@mui/material/IconButton";
import Tab from "@mui/material/Tab";
import Tabs from "@mui/material/Tabs";
import Snackbar from "@mui/material/Snackbar";
import CircularProgress from "@mui/material/CircularProgress";
import Pagination from "@mui/material/Pagination";
import Tooltip from "@mui/material/Tooltip";
import LinearProgress from "@mui/material/LinearProgress";
import FormControl from "@mui/material/FormControl";
import InputLabel from "@mui/material/InputLabel";
import Select from "@mui/material/Select";
import MenuItem from "@mui/material/MenuItem";
import RefreshIcon from "@mui/icons-material/Refresh";
import { useState } from "react";
import { useDashboardTab } from "./layout";
import {
  useStats,
  useCaches,
  useFlushCache,
  useCacheConfig,
  useNameservers,
  useListeners,
  useAddresses,
  useCreateAddress,
  useDeleteAddress,
  useForwards,
  useQueryLog,
  useTopDomains,
  useClients,
} from "@/lib/api";
import { formatUptime, formatTimestamp } from "@/lib/utils";
import { ApexOptions } from "apexcharts";

const Chart = dynamic(() => import("react-apexcharts"), { ssr: false });

function formatQueryType(qt: unknown): string {
  if (typeof qt === "string") return qt;
  if (qt && typeof qt === "object" && "Unknown" in qt) {
    return `UNKNOWN(${(qt as Record<string, unknown>).Unknown})`;
  }
  return "UNKNOWN";
}

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
    <Card sx={{ height: "100%" }}>
      <CardContent>
        <Box sx={{ display: "flex", alignItems: "center", mb: 1 }}>
          <Box
            sx={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
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
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ mt: 0.5, display: "block" }}
          >
            {subtitle}
          </Typography>
        )}
      </CardContent>
    </Card>
  );
}

function MetricCardSkeleton() {
  return (
    <Card sx={{ height: "100%" }}>
      <CardContent>
        <Skeleton variant="rounded" width={40} height={40} sx={{ mb: 1 }} />
        <Skeleton variant="text" width="60%" height={28} />
        <Skeleton variant="text" width="80%" height={48} />
      </CardContent>
    </Card>
  );
}

function TopDomainsList() {
  const { data, isLoading, error } = useTopDomains(10);
  if (isLoading) {
    return (
      <Box>
        {Array.from({ length: 5 }).map((_, i) => (
          <Skeleton key={i} variant="text" height={28} sx={{ mb: 1 }} />
        ))}
      </Box>
    );
  }
  if (error || !data?.length) {
    return (
      <Typography variant="body2" color="text.secondary">
        暂无数据
      </Typography>
    );
  }
  return (
    <List dense disablePadding>
      {data.map((d, i) => (
        <ListItem key={d.name} disableGutters sx={{ py: 0.5 }}>
          <ListItemText
            primary={
              <Typography variant="body2" sx={{ fontFamily: "monospace" }}>
                {d.name}
              </Typography>
            }
            secondary={`${d.count.toLocaleString()} 次 · 平均 ${d.avg_time_ms.toFixed(
              1
            )} ms`}
          />
          <Chip label={`#${i + 1}`} size="small" variant="outlined" />
        </ListItem>
      ))}
    </List>
  );
}

function ClientsList() {
  const { data, isLoading, error } = useClients(10);
  if (isLoading) {
    return (
      <Box>
        {Array.from({ length: 5 }).map((_, i) => (
          <Skeleton key={i} variant="text" height={28} sx={{ mb: 1 }} />
        ))}
      </Box>
    );
  }
  if (error || !data?.length) {
    return (
      <Typography variant="body2" color="text.secondary">
        暂无数据
      </Typography>
    );
  }
  return (
    <List dense disablePadding>
      {data.map((c, i) => (
        <ListItem key={c.name} disableGutters sx={{ py: 0.5 }}>
          <ListItemText
            primary={
              <Typography variant="body2" sx={{ fontFamily: "monospace" }}>
                {c.name}
              </Typography>
            }
            secondary={`${c.count.toLocaleString()} 次 · 平均 ${c.avg_time_ms.toFixed(
              1
            )} ms`}
          />
          <Chip label={`#${i + 1}`} size="small" variant="outlined" />
        </ListItem>
      ))}
    </List>
  );
}

function OverviewTab() {
  const {
    data: stats,
    isLoading: statsLoading,
    error: statsError,
  } = useStats();
  const {
    data: caches,
    isLoading: cachesLoading,
    error: cachesError,
  } = useCaches();

  const hitRateValue =
    stats != null ? `${stats.cache_hit_rate.toFixed(1)}%` : "—";
  const avgTimeValue =
    stats != null ? `${stats.avg_query_time_ms.toFixed(1)} ms` : "—";
  const cacheHitAvgTimeValue =
    stats != null ? `${stats.cache_hit_avg_query_time_ms.toFixed(1)} ms` : "—";
  const cacheMissAvgTimeValue =
    stats != null ? `${stats.cache_miss_avg_query_time_ms.toFixed(1)} ms` : "—";
  const bgAvgTimeValue =
    stats != null ? `${stats.bg_avg_query_time_ms.toFixed(1)} ms` : "—";

  const chartSeries = [
    {
      name: "总查询数",
      data: (stats?.history?.length
        ? stats.history
        : [
            {
              timestamp: 0,
              total_queries: stats?.total_queries ?? 0,
              cache_hits: stats?.cache_hits ?? 0,
            },
          ]
      ).map((p) => ({ x: p.timestamp * 1000, y: p.total_queries })),
    },
    {
      name: "缓存命中",
      data: (stats?.history?.length
        ? stats.history
        : [
            {
              timestamp: 0,
              total_queries: stats?.total_queries ?? 0,
              cache_hits: stats?.cache_hits ?? 0,
            },
          ]
      ).map((p) => ({ x: p.timestamp * 1000, y: p.cache_hits })),
    },
  ];

  const chartOptions: ApexOptions = {
    chart: {
      type: "area",
      background: "transparent",
      toolbar: { show: false },
      fontFamily: '"Inter", "Roboto", "Helvetica", "Arial", sans-serif',
    },
    theme: { mode: "dark" },
    stroke: {
      curve: "smooth",
      width: 2,
    },
    fill: {
      type: "gradient",
      gradient: {
        shadeIntensity: 1,
        opacityFrom: 0.3,
        opacityTo: 0.05,
      },
    },
    colors: ["#42a5f5", "#66bb6a"],
    xaxis: {
      type: "datetime",
      labels: {
        style: { colors: "#9ca3af" },
        datetimeUTC: false,
      },
    },
    yaxis: {
      labels: { style: { colors: "#9ca3af" } },
    },
    grid: {
      borderColor: "#1f2937",
      strokeDashArray: 4,
    },
    legend: {
      labels: { colors: "#9ca3af" },
    },
    tooltip: {
      theme: "dark",
      x: {
        format: "HH:mm:ss",
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
              title="综合平均查询时间"
              value={avgTimeValue}
              subtitle={`${stats!.cache_hit_queries.toLocaleString()} 命中 + ${stats!.cache_miss_queries.toLocaleString()} 未命中`}
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
              title="缓存命中耗时"
              value={cacheHitAvgTimeValue}
              subtitle={`${stats!.cache_hit_queries.toLocaleString()} 次命中`}
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
              title="上游查询耗时"
              value={cacheMissAvgTimeValue}
              subtitle={`${stats!.cache_miss_queries.toLocaleString()} 次未命中`}
              icon={<DnsIcon />}
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
              title="后台预取耗时"
              value={bgAvgTimeValue}
              subtitle="缓存预取平均耗时"
              icon={<TimerIcon />}
              color="#7e57c2"
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
              icon={<QueryStatsIcon />}
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
              title="后台查询数"
              value={stats!.bg_total_queries.toLocaleString()}
              subtitle="缓存预取等后台任务"
              icon={<CachedIcon />}
              color="#7e57c2"
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
              icon={<CheckCircleIcon />}
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
            <Typography
              color="text.secondary"
              sx={{ py: 4, textAlign: "center" }}
            >
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
                    <TableRow
                      key={`${entry.name}-${formatQueryType(entry.query_type)}`}
                      hover
                    >
                      <TableCell>
                        <Typography
                          variant="body2"
                          sx={{ fontFamily: "monospace" }}
                        >
                          {entry.name}
                        </Typography>
                      </TableCell>
                      <TableCell>
                        <Chip
                          label={formatQueryType(entry.query_type)}
                          size="small"
                          variant="outlined"
                        />
                      </TableCell>
                      <TableCell>{entry.hits.toLocaleString()}</TableCell>
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

      <Grid container spacing={3} sx={{ mt: 1 }}>
        <Grid size={{ xs: 12, md: 6 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 2 }}>
                <LanguageIcon sx={{ mr: 1, color: "primary.main" }} />
                <Typography variant="h6" sx={{ fontWeight: 600 }}>
                  热门域名 Top 10
                </Typography>
              </Box>
              <TopDomainsList />
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, md: 6 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 2 }}>
                <PeopleIcon sx={{ mr: 1, color: "secondary.main" }} />
                <Typography variant="h6" sx={{ fontWeight: 600 }}>
                  活跃客户端 Top 10
                </Typography>
              </Box>
              <ClientsList />
            </CardContent>
          </Card>
        </Grid>
      </Grid>
    </Box>
  );
}

function UpstreamTab() {
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

  const nsData = nameservers?.data ?? [];
  const totalQueries = nsData.reduce((s, n) => s + (n.query_total || 0), 0);
  const totalSuccess = nsData.reduce((s, n) => s + (n.success_total || 0), 0);
  const totalFailure = nsData.reduce((s, n) => s + (n.failure_total || 0), 0);
  const overallRate = totalQueries > 0 ? (totalSuccess / totalQueries) * 100 : 0;

  function statusChip(status?: string) {
    const ok = status === "ok";
    return (
      <Chip
        label={ok ? "正常" : "异常"}
        size="small"
        color={ok ? "success" : "error"}
        variant={ok ? "filled" : "outlined"}
      />
    );
  }

  function rateCell(rate: number) {
    const color =
      rate >= 90 ? "success" : rate >= 50 ? "warning" : "error";
    return (
      <Box sx={{ minWidth: 96 }}>
        <Box sx={{ display: "flex", justifyContent: "flex-end", mb: 0.5 }}>
          <Typography variant="body2">{rate.toFixed(1)}%</Typography>
        </Box>
        <LinearProgress
          variant="determinate"
          value={rate}
          color={color}
          sx={{ height: 6, borderRadius: 3 }}
        />
      </Box>
    );
  }

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 3, fontWeight: 700 }}>
        上游服务器
      </Typography>

      <Grid container spacing={3} sx={{ mb: 4 }}>
        <Grid size={{ xs: 12, sm: 6, lg: 3 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 1 }}>
                <DnsIcon sx={{ mr: 1, color: "primary.main" }} />
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
        <Grid size={{ xs: 12, sm: 6, lg: 3 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 1 }}>
                <HearingIcon sx={{ mr: 1, color: "success.main" }} />
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
        <Grid size={{ xs: 12, sm: 6, lg: 3 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 1 }}>
                <QueryStatsIcon sx={{ mr: 1, color: "info.main" }} />
                <Typography variant="body2" color="text.secondary">
                  上游总查询次数
                </Typography>
              </Box>
              {nsLoading ? (
                <Skeleton variant="text" width={80} height={48} />
              ) : (
                <Box>
                  <Typography variant="h4" sx={{ fontWeight: 700 }}>
                    {totalQueries.toLocaleString()}
                  </Typography>
                  <Typography
                    variant="caption"
                    color="text.secondary"
                    sx={{ display: "block" }}
                  >
                    成功 {totalSuccess.toLocaleString()} / 失败{" "}
                    {totalFailure.toLocaleString()}
                  </Typography>
                </Box>
              )}
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 3 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 1 }}>
                <CheckCircleIcon sx={{ mr: 1, color: "success.main" }} />
                <Typography variant="body2" color="text.secondary">
                  总体成功率
                </Typography>
              </Box>
              {nsLoading ? (
                <Skeleton variant="text" width={80} height={48} />
              ) : (
                <Typography variant="h4" sx={{ fontWeight: 700 }}>
                  {overallRate.toFixed(1)}%
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
      ) : !nsData.length ? (
        <Card>
          <CardContent>
            <Typography color="text.secondary" sx={{ textAlign: "center" }}>
              暂无上游服务器配置
            </Typography>
          </CardContent>
        </Card>
      ) : (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>分组</TableCell>
                <TableCell>名称</TableCell>
                <TableCell>地址</TableCell>
                <TableCell>IP:端口</TableCell>
                <TableCell>协议</TableCell>
                <TableCell>安全</TableCell>
                <TableCell>状态</TableCell>
                <TableCell align="right">查询</TableCell>
                <TableCell align="right">成功</TableCell>
                <TableCell align="right">失败</TableCell>
                <TableCell align="right">平均耗时</TableCell>
                <TableCell>成功率</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {nsData.map((ns, idx) => (
                <TableRow key={idx} hover>
                  <TableCell>
                    <Typography variant="body2" sx={{ fontWeight: 500 }}>
                      {ns.group?.length ? ns.group.join(", ") : "-"}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    {ns.name ? (
                      <Typography variant="body2" sx={{ fontWeight: 500 }}>
                        {ns.name}
                      </Typography>
                    ) : (
                      <Typography variant="body2" color="text.secondary">
                        -
                      </Typography>
                    )}
                  </TableCell>
                  <TableCell>
                    <Typography
                      variant="body2"
                      sx={{ fontFamily: "monospace" }}
                    >
                      {ns.server}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    <Typography
                      variant="body2"
                      sx={{ fontFamily: "monospace" }}
                    >
                      {ns.ip}:{ns.port}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    <Chip
                      label={ns.protocol || "未知"}
                      size="small"
                      color="primary"
                      variant="outlined"
                    />
                  </TableCell>
                  <TableCell>
                    <Chip
                      label={ns.security || "明文"}
                      size="small"
                      color={ns.security === "加密" ? "secondary" : "default"}
                      variant="outlined"
                    />
                  </TableCell>
                  <TableCell>{statusChip(ns.status)}</TableCell>
                  <TableCell align="right">
                    {ns.query_total.toLocaleString()}
                  </TableCell>
                  <TableCell align="right">
                    {ns.success_total.toLocaleString()}
                  </TableCell>
                  <TableCell align="right">
                    {ns.failure_total.toLocaleString()}
                  </TableCell>
                  <TableCell align="right">
                    {ns.avg_time_ms.toFixed(1)} ms
                  </TableCell>
                  <TableCell>{rateCell(ns.success_rate)}</TableCell>
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
            <Typography color="text.secondary" sx={{ textAlign: "center" }}>
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
                    <Typography
                      variant="body2"
                      sx={{ fontFamily: "monospace" }}
                    >
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

function CacheTab() {
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const rowsPerPage = 50;
  const offset = (page - 1) * rowsPerPage;

  const [snackbar, setSnackbar] = useState<{
    open: boolean;
    message: string;
    severity: "success" | "error";
  }>({ open: false, message: "", severity: "success" });

  const {
    data: caches,
    isLoading: cachesLoading,
    error: cachesError,
  } = useCaches(offset, rowsPerPage);

  const { data: cacheConfig, isLoading: configLoading } = useCacheConfig();

  const flushCache = useFlushCache();

  const handleFlush = () => {
    flushCache.mutate(undefined, {
      onSuccess: () => {
        setPage(1);
        setSnackbar({
          open: true,
          message: "缓存已清空",
          severity: "success",
        });
      },
      onError: () => {
        setSnackbar({
          open: true,
          message: "清空缓存失败",
          severity: "error",
        });
      },
    });
  };

  const filteredCaches =
    caches?.data?.filter((entry) =>
      entry.name.toLowerCase().includes(search.toLowerCase()),
    ) ?? [];

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 3, fontWeight: 700 }}>
        缓存管理
      </Typography>

      <Grid container spacing={3} sx={{ mb: 4 }}>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 1 }}>
                <StorageIcon sx={{ mr: 1, color: "primary.main" }} />
                <Typography variant="body2" color="text.secondary">
                  缓存大小限制
                </Typography>
              </Box>
              {configLoading ? (
                <Skeleton variant="text" width={80} height={48} />
              ) : (
                <Typography variant="h4" sx={{ fontWeight: 700 }}>
                  {cacheConfig?.size ?? "-"}
                </Typography>
              )}
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          <Card sx={{ height: "100%" }}>
            <CardContent>
              <Box sx={{ display: "flex", alignItems: "center", mb: 1 }}>
                <StorageIcon sx={{ mr: 1, color: "warning.main" }} />
                <Typography variant="body2" color="text.secondary">
                  当前缓存条目
                </Typography>
              </Box>
              {cachesLoading ? (
                <Skeleton variant="text" width={80} height={48} />
              ) : (
                <Typography variant="h4" sx={{ fontWeight: 700 }}>
                  {caches?.total ?? 0}
                </Typography>
              )}
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, lg: 4 }}>
          <Card sx={{ height: "100%", display: "flex", alignItems: "center" }}>
            <CardContent sx={{ width: "100%" }}>
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
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              mb: 2,
              flexWrap: "wrap",
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
              sx={{ py: 4, textAlign: "center" }}
            >
              {search ? "未找到匹配的缓存条目" : "暂无缓存条目"}
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
                    <TableRow
                      key={`${entry.name}-${formatQueryType(entry.query_type)}`}
                      hover
                    >
                      <TableCell>
                        <Typography
                          variant="body2"
                          sx={{ fontFamily: "monospace" }}
                        >
                          {entry.name}
                        </Typography>
                      </TableCell>
                      <TableCell>
                        <Chip
                          label={formatQueryType(entry.query_type)}
                          size="small"
                          variant="outlined"
                        />
                      </TableCell>
                      <TableCell>{entry.hits.toLocaleString()}</TableCell>
                      <TableCell>
                        {formatTimestamp(entry.last_access)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          )}
          {caches && caches.total > rowsPerPage && (
            <Box sx={{ display: "flex", justifyContent: "center", mt: 2 }}>
              <Pagination
                count={Math.ceil((caches.total || 0) / rowsPerPage)}
                page={page}
                onChange={(_, newPage) => setPage(newPage)}
                color="primary"
              />
            </Box>
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

interface TabPanelProps {
  children: React.ReactNode;
  value: number;
  index: number;
}

function RulesTabPanel({ children, value, index }: TabPanelProps) {
  return (
    <Box role="tabpanel" hidden={value !== index} sx={{ pt: 3 }}>
      {value === index && children}
    </Box>
  );
}

function AddressRulesTab() {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [domain, setDomain] = useState("");
  const [address, setAddress] = useState("");
  const [snackbar, setSnackbar] = useState<{
    open: boolean;
    message: string;
    severity: "success" | "error";
  }>({ open: false, message: "", severity: "success" });

  const { data: addresses, isLoading, error } = useAddresses();

  const createAddress = useCreateAddress();
  const deleteAddress = useDeleteAddress();

  const handleAdd = () => {
    if (!domain || !address) return;
    createAddress.mutate(
      { domain, address },
      {
        onSuccess: () => {
          setSnackbar({
            open: true,
            message: "地址规则添加成功",
            severity: "success",
          });
          setDialogOpen(false);
          setDomain("");
          setAddress("");
        },
        onError: () => {
          setSnackbar({
            open: true,
            message: "添加地址规则失败",
            severity: "error",
          });
        },
      },
    );
  };

  const handleDelete = (d: string) => {
    deleteAddress.mutate(d, {
      onSuccess: () => {
        setSnackbar({
          open: true,
          message: "地址规则已删除",
          severity: "success",
        });
      },
      onError: () => {
        setSnackbar({
          open: true,
          message: "删除地址规则失败",
          severity: "error",
        });
      },
    });
  };

  return (
    <Box>
      <Box
        sx={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
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
            <Typography color="text.secondary" sx={{ textAlign: "center" }}>
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
                    <Typography
                      variant="body2"
                      sx={{ fontFamily: "monospace" }}
                    >
                      {entry.domain}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    <Chip
                      label={entry.address}
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
            label="地址"
            fullWidth
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="1.2.3.4"
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDialogOpen(false)}>取消</Button>
          <Button
            variant="contained"
            onClick={handleAdd}
            disabled={createAddress.isPending || !domain || !address}
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
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
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
            <Typography color="text.secondary" sx={{ textAlign: "center" }}>
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
                  display: "flex",
                  alignItems: "center",
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
                              sx={{ fontFamily: "monospace" }}
                            >
                              {typeof fw === "string" ? fw : JSON.stringify(fw)}
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

function RulesTab() {
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
            sx={{ borderBottom: 1, borderColor: "divider" }}
          >
            <Tab label="地址规则" />
            <Tab label="转发规则" />
          </Tabs>

          <RulesTabPanel value={tab} index={0}>
            <AddressRulesTab />
          </RulesTabPanel>
          <RulesTabPanel value={tab} index={1}>
            <ForwardRulesTab />
          </RulesTabPanel>
        </CardContent>
      </Card>
    </Box>
  );
}

function QueryLogTab() {
  const [domain, setDomain] = useState("");
  const [client, setClient] = useState("");
  const [qtype, setQtype] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);

  const { data, isLoading, error, refetch } = useQueryLog(
    {
      limit: 100,
      domain: domain || undefined,
      client: client || undefined,
      qtype: qtype || undefined,
    },
    autoRefresh
  );

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 2, fontWeight: 700 }}>
        查询日志
      </Typography>
      <Typography variant="body2" color="text.secondary" sx={{ mb: 3 }}>
        展示最近 1000 条查询记录（内存缓存，服务重启后清空）。
      </Typography>

      <Paper variant="outlined" sx={{ p: 2, mb: 3 }}>
        <Grid container spacing={2} alignItems="center">
          <Grid size={{ xs: 12, sm: 4 }}>
            <TextField
              fullWidth
              size="small"
              label="域名过滤"
              value={domain}
              onChange={(e) => setDomain(e.target.value)}
              placeholder="如 example.com"
            />
          </Grid>
          <Grid size={{ xs: 12, sm: 4 }}>
            <TextField
              fullWidth
              size="small"
              label="客户端过滤"
              value={client}
              onChange={(e) => setClient(e.target.value)}
              placeholder="如 192.168.1.1"
            />
          </Grid>
          <Grid size={{ xs: 12, sm: 2 }}>
            <FormControl fullWidth size="small">
              <InputLabel>类型</InputLabel>
              <Select
                label="类型"
                value={qtype}
                onChange={(e) => setQtype(e.target.value)}
              >
                <MenuItem value="">全部</MenuItem>
                <MenuItem value="A">A</MenuItem>
                <MenuItem value="AAAA">AAAA</MenuItem>
                <MenuItem value="CNAME">CNAME</MenuItem>
                <MenuItem value="MX">MX</MenuItem>
                <MenuItem value="TXT">TXT</MenuItem>
                <MenuItem value="PTR">PTR</MenuItem>
                <MenuItem value="HTTPS">HTTPS</MenuItem>
                <MenuItem value="SRV">SRV</MenuItem>
              </Select>
            </FormControl>
          </Grid>
          <Grid size={{ xs: 12, sm: 2 }}>
            <Box sx={{ display: "flex", gap: 1, alignItems: "center" }}>
              <Tooltip title={autoRefresh ? "暂停自动刷新" : "开启自动刷新"}>
                <IconButton
                  color={autoRefresh ? "primary" : "default"}
                  onClick={() => setAutoRefresh((v) => !v)}
                >
                  {autoRefresh ? <PauseIcon /> : <PlayArrowIcon />}
                </IconButton>
              </Tooltip>
              <Tooltip title="立即刷新">
                <IconButton onClick={() => refetch()}>
                  <RefreshIcon />
                </IconButton>
              </Tooltip>
            </Box>
          </Grid>
        </Grid>
      </Paper>

      {isLoading ? (
        <Box>
          {Array.from({ length: 8 }).map((_, i) => (
            <Skeleton key={i} variant="rounded" height={36} sx={{ mb: 1 }} />
          ))}
        </Box>
      ) : error ? (
        <Alert severity="error">加载查询日志失败</Alert>
      ) : !data?.data?.length ? (
        <Card>
          <CardContent>
            <Typography color="text.secondary" sx={{ textAlign: "center" }}>
              暂无查询日志
            </Typography>
          </CardContent>
        </Card>
      ) : (
        <>
          <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
            共 {data.total} 条，当前显示 {data.data.length} 条
          </Typography>
          <TableContainer component={Paper} variant="outlined">
            <Table size="small" stickyHeader>
              <TableHead>
                <TableRow>
                  <TableCell>时间</TableCell>
                  <TableCell>客户端</TableCell>
                  <TableCell>域名</TableCell>
                  <TableCell>类型</TableCell>
                  <TableCell align="right">耗时</TableCell>
                  <TableCell>结果</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {data.data.map((e, idx) => (
                  <TableRow key={idx} hover>
                    <TableCell>
                      <Typography
                        variant="body2"
                        sx={{ whiteSpace: "nowrap" }}
                      >
                        {formatTimestamp(e.time)}
                      </Typography>
                    </TableCell>
                    <TableCell>
                      <Typography
                        variant="body2"
                        sx={{ fontFamily: "monospace" }}
                      >
                        {e.client}
                      </Typography>
                    </TableCell>
                    <TableCell>
                      <Typography variant="body2">{e.domain}</Typography>
                    </TableCell>
                    <TableCell>
                      <Chip
                        label={e.query_type}
                        size="small"
                        variant="outlined"
                      />
                    </TableCell>
                    <TableCell align="right">
                      {e.elapsed_ms.toFixed(1)} ms
                    </TableCell>
                    <TableCell>
                      <Chip
                        label={e.result === "success" ? "成功" : "失败"}
                        size="small"
                        color={e.result === "success" ? "success" : "error"}
                        variant={e.result === "success" ? "filled" : "outlined"}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        </>
      )}
    </Box>
  );
}

export default function DashboardPage() {
  const { currentTab } = useDashboardTab();

  switch (currentTab) {
    case "upstream":
      return <UpstreamTab />;
    case "cache":
      return <CacheTab />;
    case "rules":
      return <RulesTab />;
    case "querylog":
      return <QueryLogTab />;
    case "overview":
    default:
      return <OverviewTab />;
  }
}
