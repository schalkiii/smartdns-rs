"use client";

import { useState } from "react";
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
import Button from "@mui/material/Button";
import TextField from "@mui/material/TextField";
import MenuItem from "@mui/material/MenuItem";
import Dialog from "@mui/material/Dialog";
import DialogActions from "@mui/material/DialogActions";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import IconButton from "@mui/material/IconButton";
import Snackbar from "@mui/material/Snackbar";
import Tooltip from "@mui/material/Tooltip";
import AddIcon from "@mui/icons-material/Add";
import DeleteIcon from "@mui/icons-material/Delete";
import EditIcon from "@mui/icons-material/Edit";
import BlockIcon from "@mui/icons-material/Block";
import ShieldIcon from "@mui/icons-material/Shield";
import RuleIcon from "@mui/icons-material/Rule";
import {
  useDomainSets,
  useCreateDomainSet,
  useUpdateDomainSet,
  useDeleteDomainSet,
  useAdblockStats,
} from "@/lib/api";

interface SnackbarState {
  open: boolean;
  message: string;
  severity: "success" | "error";
}

export default function AdblockTab() {
  const [addOpen, setAddOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<{
    name: string;
    url: string;
    interval?: number | null;
  } | null>(null);
  const [snackbar, setSnackbar] = useState<SnackbarState>({
    open: false,
    message: "",
    severity: "success",
  });

  const { data: sets, isLoading, error } = useDomainSets();
  const { data: stats } = useAdblockStats();

  const createSet = useCreateDomainSet();
  const updateSet = useUpdateDomainSet();
  const deleteSet = useDeleteDomainSet();

  const totalRules =
    sets?.data?.reduce((sum, s) => sum + s.rule_count, 0) ?? 0;
  const totalAllow =
    sets?.data?.reduce((sum, s) => sum + s.allow_count, 0) ?? 0;

  return (
    <Box>
      {/* 统计卡片 */}
      <Grid container spacing={2} sx={{ mb: 3 }}>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
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
                    bgcolor: "error.main" + "22",
                    mr: 1.5,
                  }}
                >
                  <BlockIcon color="error" />
                </Box>
                <Typography
                  variant="body2"
                  color="text.secondary"
                >
                  累计拦截
                </Typography>
              </Box>
              <Typography variant="h5" sx={{ fontWeight: 700 }}>
                {stats?.blocked_total?.toLocaleString() ?? "—"}
              </Typography>
              <Typography
                variant="caption"
                color="text.secondary"
              >
                自启动以来 address 规则 + 订阅命中
              </Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
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
                    bgcolor: "primary.main" + "22",
                    mr: 1.5,
                  }}
                >
                  <ShieldIcon color="primary" />
                </Box>
                <Typography variant="body2" color="text.secondary">
                  活跃订阅
                </Typography>
              </Box>
              <Typography variant="h5" sx={{ fontWeight: 700 }}>
                {sets?.count ?? "—"}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                已加载的 domain-set 订阅数
              </Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
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
                    bgcolor: "warning.main" + "22",
                    mr: 1.5,
                  }}
                >
                  <RuleIcon color="warning" />
                </Box>
                <Typography variant="body2" color="text.secondary">
                  拦截规则数
                </Typography>
              </Box>
              <Typography variant="h5" sx={{ fontWeight: 700 }}>
                {totalRules.toLocaleString()}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                所有订阅的 block 集条目总和
              </Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
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
                    bgcolor: "success.main" + "22",
                    mr: 1.5,
                  }}
                >
                  <BlockIcon color="success" />
                </Box>
                <Typography variant="body2" color="text.secondary">
                  例外规则数
                </Typography>
              </Box>
              <Typography variant="h5" sx={{ fontWeight: 700 }}>
                {totalAllow.toLocaleString()}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                @@ 例外（adblock 格式）总和
              </Typography>
            </CardContent>
          </Card>
        </Grid>
      </Grid>

      {/* 订阅列表 */}
      <Box
        sx={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          mb: 2,
        }}
      >
        <Typography variant="h6" sx={{ fontWeight: 600 }}>
          订阅列表 ({sets?.count ?? 0})
        </Typography>
        <Button
          variant="contained"
          startIcon={<AddIcon />}
          onClick={() => setAddOpen(true)}
        >
          添加订阅
        </Button>
      </Box>

      {isLoading ? (
        <Box>
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} variant="rounded" height={56} sx={{ mb: 1 }} />
          ))}
        </Box>
      ) : error ? (
        <Alert severity="error">加载订阅列表失败</Alert>
      ) : !sets?.data?.length ? (
        <Card variant="outlined">
          <CardContent>
            <Typography color="text.secondary" sx={{ textAlign: "center" }}>
              暂无订阅。点击「添加订阅」接入 adblock / list 格式的规则源，
              并配合 address /domain-set:名称/# 实现拦截。
            </Typography>
          </CardContent>
        </Card>
      ) : (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>名称</TableCell>
                <TableCell>类型</TableCell>
                <TableCell>来源</TableCell>
                <TableCell>订阅地址</TableCell>
                <TableCell align="right">规则数</TableCell>
                <TableCell align="right">例外数</TableCell>
                <TableCell align="right">操作</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {sets.data.map((entry) => (
                <TableRow key={entry.name} hover>
                  <TableCell>
                    <Typography variant="body2" sx={{ fontWeight: 600 }}>
                      {entry.name}
                    </Typography>
                  </TableCell>
                  <TableCell>
                    <Chip
                      size="small"
                      color={entry.content_type === "adblock" ? "error" : "default"}
                      variant="outlined"
                      label={entry.content_type || "list"}
                    />
                  </TableCell>
                  <TableCell>
                    <Chip
                      size="small"
                      variant="outlined"
                      label={entry.source || "—"}
                    />
                  </TableCell>
                  <TableCell
                    sx={{
                      maxWidth: 320,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    <Tooltip title={entry.location}>
                      <Typography variant="body2" noWrap component="span">
                        {entry.location || "—"}
                      </Typography>
                    </Tooltip>
                  </TableCell>
                  <TableCell align="right">
                    <Typography variant="body2">
                      {entry.rule_count.toLocaleString()}
                    </Typography>
                  </TableCell>
                  <TableCell align="right">
                    <Typography variant="body2">
                      {entry.allow_count > 0
                        ? entry.allow_count.toLocaleString()
                        : "—"}
                    </Typography>
                  </TableCell>
                  <TableCell align="right">
                    <Box sx={{ display: "flex", gap: 0.5, justifyContent: "flex-end" }}>
                      {entry.source === "http" && (
                        <Tooltip title="编辑订阅">
                          <IconButton
                            size="small"
                            onClick={() =>
                              setEditTarget({
                                name: entry.name,
                                url: entry.location,
                                interval: entry.interval,
                              })
                            }
                          >
                            <EditIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                      )}
                      <Tooltip title="删除订阅">
                        <IconButton
                          size="small"
                          color="error"
                          onClick={() => {
                            deleteSet.mutate(entry.name, {
                              onSuccess: () =>
                                setSnackbar({
                                  open: true,
                                  message: `订阅 ${entry.name} 已删除`,
                                  severity: "success",
                                }),
                              onError: () =>
                                setSnackbar({
                                  open: true,
                                  message: "删除订阅失败",
                                  severity: "error",
                                }),
                            });
                          }}
                        >
                          <DeleteIcon fontSize="small" />
                        </IconButton>
                      </Tooltip>
                    </Box>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      )}

      {/* Top 拦截域名 / 客户端 */}
      {stats && (stats.top_blocked_domains.length > 0 || stats.top_blocked_clients.length > 0) && (
        <Grid container spacing={2} sx={{ mt: 3 }}>
          <Grid size={{ xs: 12, md: 6 }}>
            <Card variant="outlined">
              <CardContent>
                <Typography variant="subtitle1" sx={{ fontWeight: 600, mb: 1 }}>
                  Top 拦截域名（最近 1000 条查询）
                </Typography>
                {stats.top_blocked_domains.length === 0 ? (
                  <Typography variant="body2" color="text.secondary">
                    暂无拦截记录
                  </Typography>
                ) : (
                  stats.top_blocked_domains.map((d) => (
                    <Box
                      key={d.name}
                      sx={{
                        display: "flex",
                        justifyContent: "space-between",
                        py: 0.5,
                      }}
                    >
                      <Typography variant="body2" noWrap sx={{ pr: 2 }}>
                        {d.name}
                      </Typography>
                      <Chip size="small" color="error" label={d.count} />
                    </Box>
                  ))
                )}
              </CardContent>
            </Card>
          </Grid>
          <Grid size={{ xs: 12, md: 6 }}>
            <Card variant="outlined">
              <CardContent>
                <Typography variant="subtitle1" sx={{ fontWeight: 600, mb: 1 }}>
                  Top 拦截触发客户端（最近 1000 条查询）
                </Typography>
                {stats.top_blocked_clients.length === 0 ? (
                  <Typography variant="body2" color="text.secondary">
                    暂无拦截记录
                  </Typography>
                ) : (
                  stats.top_blocked_clients.map((c) => (
                    <Box
                      key={c.name}
                      sx={{
                        display: "flex",
                        justifyContent: "space-between",
                        py: 0.5,
                      }}
                    >
                      <Typography variant="body2" noWrap sx={{ pr: 2 }}>
                        {c.name}
                      </Typography>
                      <Chip size="small" color="error" label={c.count} />
                    </Box>
                  ))
                )}
              </CardContent>
            </Card>
          </Grid>
        </Grid>
      )}

      {/* 添加订阅 Dialog */}
      <AddSubscriptionDialog
        open={addOpen}
        onClose={() => setAddOpen(false)}
        onSubmit={(input) => {
          createSet.mutate(input, {
            onSuccess: () => {
              setSnackbar({
                open: true,
                message: "订阅添加成功",
                severity: "success",
              });
              setAddOpen(false);
            },
            onError: (e: Error) => {
              setSnackbar({
                open: true,
                message: `添加订阅失败: ${e.message}`,
                severity: "error",
              });
            },
          });
        }}
        submitting={createSet.isPending}
      />

      {/* 编辑订阅 Dialog */}
      <EditSubscriptionDialog
        target={editTarget}
        onClose={() => setEditTarget(null)}
        onSubmit={(input) => {
          if (!editTarget) return;
          updateSet.mutate(
            { name: editTarget.name, url: input.url, interval: input.interval },
            {
              onSuccess: () => {
                setSnackbar({
                  open: true,
                  message: "订阅更新成功",
                  severity: "success",
                });
                setEditTarget(null);
              },
              onError: (e: Error) => {
                setSnackbar({
                  open: true,
                  message: `更新订阅失败: ${e.message}`,
                  severity: "error",
                });
              },
            },
          );
        }}
        submitting={updateSet.isPending}
      />

      {/* 操作反馈 */}
      <Snackbar
        open={snackbar.open}
        autoHideDuration={4000}
        onClose={() => setSnackbar({ ...snackbar, open: false })}
        anchorOrigin={{ vertical: "bottom", horizontal: "center" }}
      >
        <Alert
          severity={snackbar.severity}
          onClose={() => setSnackbar({ ...snackbar, open: false })}
        >
          {snackbar.message}
        </Alert>
      </Snackbar>
    </Box>
  );
}

interface SubscriptionInput {
  name: string;
  url: string;
  contentType: string;
  interval?: number | null;
}

function AddSubscriptionDialog({
  open,
  onClose,
  onSubmit,
  submitting,
}: {
  open: boolean;
  onClose: () => void;
  onSubmit: (input: {
    name: string;
    url: string;
    content_type: string;
    interval?: number | null;
  }) => void;
  submitting: boolean;
}) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [contentType, setContentType] = useState("adblock");
  const [interval, setInterval] = useState("");

  const valid = name.trim().length > 0 && url.trim().length > 0;

  const preset =
    "https://raw.githubusercontent.com/TG-Twilight/AWAvenue-Ads-Rule/main/Filters/AWAvenue-Ads-Rule-DNS.txt";

  return (
    <Dialog open={open} onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>添加订阅</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="订阅名称"
          fullWidth
          variant="outlined"
          value={name}
          onChange={(e) => setName(e.target.value)}
          helperText="配置中的 -name，供 address /domain-set:名称/# 引用"
          sx={{ mb: 1 }}
        />
        <TextField
          margin="dense"
          label="订阅 URL"
          fullWidth
          variant="outlined"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          helperText="规则文件的下载地址"
          sx={{ mb: 1 }}
        />
        <TextField
          margin="dense"
          select
          label="内容格式"
          fullWidth
          variant="outlined"
          value={contentType}
          onChange={(e) => setContentType(e.target.value)}
          helperText="adblock: ||domain^ / @@例外 / hosts 行；list: 每行一个域名"
          sx={{ mb: 1 }}
        >
          <MenuItem value="adblock">adblock（AdGuard/ABP 格式）</MenuItem>
          <MenuItem value="list">list（纯域名列表）</MenuItem>
        </TextField>
        <TextField
          margin="dense"
          label="自动更新间隔（秒，可选）"
          fullWidth
          variant="outlined"
          value={interval}
          onChange={(e) =>
            setInterval(e.target.value.replace(/[^0-9]/g, ""))
          }
          placeholder="如 86400（每天）"
        />
        <Button
          size="small"
          sx={{ mt: 1 }}
          onClick={() => {
            setName("ads");
            setUrl(preset);
            setContentType("adblock");
          }}
        >
          填入 AWAvenue 广告规则示例
        </Button>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>取消</Button>
        <Button
          variant="contained"
          disabled={!valid || submitting}
          onClick={() => {
            onSubmit({
              name: name.trim(),
              url: url.trim(),
              content_type: contentType,
              interval: interval ? Number(interval) : null,
            });
            setName("");
            setUrl("");
            setContentType("adblock");
            setInterval("");
          }}
        >
          {submitting ? "添加中…" : "添加"}
        </Button>
      </DialogActions>
    </Dialog>
  );
}

function EditSubscriptionDialog({
  target,
  onClose,
  onSubmit,
  submitting,
}: {
  target: { name: string; url: string; interval?: number | null } | null;
  onClose: () => void;
  onSubmit: (input: SubscriptionInput) => void;
  submitting: boolean;
}) {
  const [url, setUrl] = useState("");
  const [interval, setInterval] = useState("");
  const [initialized, setInitialized] = useState(false);

  if (target && !initialized) {
    setUrl(target.url);
    setInterval(target.interval ? String(target.interval) : "");
    setInitialized(true);
  }
  if (!target && initialized) {
    setInitialized(false);
  }

  return (
    <Dialog
      open={target !== null}
      onClose={onClose}
      maxWidth="sm"
      fullWidth
    >
      <DialogTitle>编辑订阅：{target?.name}</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="订阅 URL"
          fullWidth
          variant="outlined"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          sx={{ mb: 1 }}
        />
        <TextField
          margin="dense"
          label="自动更新间隔（秒，可选）"
          fullWidth
          variant="outlined"
          value={interval}
          onChange={(e) =>
            setInterval(e.target.value.replace(/[^0-9]/g, ""))
          }
          placeholder="如 86400（每天）"
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>取消</Button>
        <Button
          variant="contained"
          disabled={!url.trim() || submitting}
          onClick={() => {
            onSubmit({
              name: target?.name ?? "",
              url: url.trim(),
              contentType: "",
              interval: interval ? Number(interval) : null,
            });
          }}
        >
          {submitting ? "保存中…" : "保存"}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
