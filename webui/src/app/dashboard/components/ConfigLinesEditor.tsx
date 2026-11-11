'use client';

// 通用配置行编辑器：按家族（server/bind/forward/…）列出主配置中的多实例键，
// 支持增删改，写回后自动热更新。供设置页高级区与上游/规则页复用。

import { useState } from 'react';
import Box from '@mui/material/Box';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import Typography from '@mui/material/Typography';
import Table from '@mui/material/Table';
import TableBody from '@mui/material/TableBody';
import TableCell from '@mui/material/TableCell';
import TableContainer from '@mui/material/TableContainer';
import TableHead from '@mui/material/TableHead';
import TableRow from '@mui/material/TableRow';
import Paper from '@mui/material/Paper';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import Chip from '@mui/material/Chip';
import Alert from '@mui/material/Alert';
import Skeleton from '@mui/material/Skeleton';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import TextField from '@mui/material/TextField';
import FormControl from '@mui/material/FormControl';
import InputLabel from '@mui/material/InputLabel';
import Select from '@mui/material/Select';
import MenuItem from '@mui/material/MenuItem';
import Snackbar from '@mui/material/Snackbar';
import CircularProgress from '@mui/material/CircularProgress';
import AddIcon from '@mui/icons-material/Add';
import EditIcon from '@mui/icons-material/Edit';
import DeleteIcon from '@mui/icons-material/Delete';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import {
  useConfigLines,
  useMutateConfigLines,
  type ConfigLineEntry,
} from '@/lib/api';
import { FAMILY_DEFS } from '@/lib/config-catalog';

interface SnackbarState {
  open: boolean;
  message: string;
  severity: 'success' | 'error';
}

interface EditState {
  mode: 'add' | 'edit';
  index?: number;
  key: string;
  value: string;
  comment: string;
}

export default function ConfigLinesEditor({ family }: { family: string }) {
  const def = FAMILY_DEFS[family];
  const { data, isLoading, error } = useConfigLines(family);
  const mutate = useMutateConfigLines();
  const [edit, setEdit] = useState<EditState | null>(null);
  const [deleting, setDeleting] = useState<ConfigLineEntry | null>(null);
  const [snackbar, setSnackbar] = useState<SnackbarState>({
    open: false,
    message: '',
    severity: 'success',
  });

  if (!def) return null;

  const lines = data?.lines ?? [];
  const multiKey = def.keys.length > 1;

  function openAdd() {
    setEdit({ mode: 'add', key: def.keys[0].value, value: '', comment: '' });
  }

  function openEdit(entry: ConfigLineEntry) {
    setEdit({
      mode: 'edit',
      index: entry.index,
      key: entry.key,
      value: entry.value,
      comment: entry.comment ?? '',
    });
  }

  function submitEdit() {
    if (!edit) return;
    mutate.mutate(
      {
        family,
        action: edit.mode === 'add' ? 'add' : 'update',
        key: edit.key,
        value: edit.value.trim(),
        index: edit.index,
        comment: edit.comment.trim() || null,
      },
      {
        onSuccess: () => {
          setEdit(null);
          setSnackbar({
            open: true,
            message: '已保存并热更新配置',
            severity: 'success',
          });
        },
        onError: (e: Error) => {
          setSnackbar({ open: true, message: e.message, severity: 'error' });
        },
      }
    );
  }

  function confirmDelete() {
    if (!deleting) return;
    mutate.mutate(
      {
        family,
        action: 'delete',
        key: deleting.key,
        index: deleting.index,
      },
      {
        onSuccess: () => {
          setDeleting(null);
          setSnackbar({
            open: true,
            message: '已删除并热更新配置',
            severity: 'success',
          });
        },
        onError: (e: Error) => {
          setSnackbar({ open: true, message: e.message, severity: 'error' });
        },
      }
    );
  }

  return (
    <Card variant="outlined" sx={{ mb: 3 }}>
      <CardContent>
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            mb: 1,
          }}
        >
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
            <Typography variant="h6" sx={{ fontWeight: 600 }}>
              {def.title}
            </Typography>
            <Tooltip
              title={
                <Box>
                  <Typography variant="body2">{def.description}</Typography>
                  <Typography variant="body2" sx={{ mt: 1 }}>
                    {def.valueHelp}
                  </Typography>
                </Box>
              }
            >
              <InfoOutlinedIcon
                fontSize="small"
                color="action"
                tabIndex={0}
                role="img"
                aria-label={`${def.title} 配置说明`}
              />
            </Tooltip>
          </Box>
          <Button
            size="small"
            startIcon={<AddIcon />}
            variant="outlined"
            onClick={openAdd}
          >
            添加
          </Button>
        </Box>

        {isLoading ? (
          <Box>
            {Array.from({ length: 2 }).map((_, i) => (
              <Skeleton key={i} variant="rounded" height={40} sx={{ mb: 1 }} />
            ))}
          </Box>
        ) : error ? (
          <Alert severity="error">加载配置行失败：{String(error)}</Alert>
        ) : lines.length === 0 ? (
          <Typography color="text.secondary" sx={{ py: 2, textAlign: 'center' }}>
            暂无配置项
          </Typography>
        ) : (
          <TableContainer component={Paper} variant="outlined">
            <Table size="small">
              <TableHead>
                <TableRow>
                  {multiKey && <TableCell>指令</TableCell>}
                  <TableCell>值</TableCell>
                  <TableCell>注释</TableCell>
                  <TableCell align="right">操作</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {lines.map((entry) => (
                  <TableRow key={entry.index} hover>
                    {multiKey && (
                      <TableCell>
                        <Chip
                          label={entry.key}
                          size="small"
                          variant="outlined"
                          color="primary"
                        />
                      </TableCell>
                    )}
                    <TableCell>
                      <Typography
                        variant="body2"
                        sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }}
                      >
                        {entry.value || '（无值）'}
                      </Typography>
                    </TableCell>
                    <TableCell>
                      <Typography variant="body2" color="text.secondary">
                        {entry.comment ?? '-'}
                      </Typography>
                    </TableCell>
                    <TableCell align="right">
                      <Tooltip title="编辑">
                        <IconButton size="small" onClick={() => openEdit(entry)}>
                          <EditIcon fontSize="small" />
                        </IconButton>
                      </Tooltip>
                      <Tooltip title="删除">
                        <IconButton
                          size="small"
                          color="error"
                          onClick={() => setDeleting(entry)}
                        >
                          <DeleteIcon fontSize="small" />
                        </IconButton>
                      </Tooltip>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        )}
      </CardContent>

      {/* 添加 / 编辑 Dialog */}
      <Dialog
        open={edit !== null}
        onClose={() => !mutate.isPending && setEdit(null)}
        maxWidth="sm"
        fullWidth
      >
        <DialogTitle>
          {edit?.mode === 'add' ? `添加${def.title}` : `编辑${def.title}`}
        </DialogTitle>
        <DialogContent>
          <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, mt: 1 }}>
            {multiKey && (
              <FormControl fullWidth size="small">
                <InputLabel>指令</InputLabel>
                <Select
                  label="指令"
                  value={edit?.key ?? ''}
                  onChange={(e) =>
                    setEdit((s) => (s ? { ...s, key: e.target.value } : s))
                  }
                >
                  {def.keys.map((k) => (
                    <MenuItem key={k.value} value={k.value}>
                      {k.label}
                    </MenuItem>
                  ))}
                </Select>
              </FormControl>
            )}
            <TextField
              fullWidth
              size="small"
              label="值"
              value={edit?.value ?? ''}
              onChange={(e) =>
                setEdit((s) => (s ? { ...s, value: e.target.value } : s))
              }
              placeholder={def.valuePlaceholder}
              helperText={def.valueHelp}
              slotProps={{ htmlInput: { style: { fontFamily: 'monospace' } } }}
            />
            <TextField
              fullWidth
              size="small"
              label="注释（可选）"
              value={edit?.comment ?? ''}
              onChange={(e) =>
                setEdit((s) => (s ? { ...s, comment: e.target.value } : s))
              }
              placeholder="写在该行末尾的 # 注释"
            />
          </Box>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setEdit(null)} disabled={mutate.isPending}>
            取消
          </Button>
          <Button
            variant="contained"
            onClick={submitEdit}
            disabled={mutate.isPending || !edit?.key}
            startIcon={
              mutate.isPending ? <CircularProgress size={16} /> : undefined
            }
          >
            保存并热更新
          </Button>
        </DialogActions>
      </Dialog>

      {/* 删除确认 Dialog */}
      <Dialog
        open={deleting !== null}
        onClose={() => !mutate.isPending && setDeleting(null)}
        maxWidth="xs"
        fullWidth
      >
        <DialogTitle>确认删除</DialogTitle>
        <DialogContent>
          <Typography variant="body2" sx={{ mb: 1 }}>
            将从主配置文件删除以下行并热更新：
          </Typography>
          <Typography
            variant="body2"
            sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }}
          >
            {deleting?.key} {deleting?.value}
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDeleting(null)} disabled={mutate.isPending}>
            取消
          </Button>
          <Button
            color="error"
            variant="contained"
            onClick={confirmDelete}
            disabled={mutate.isPending}
            startIcon={
              mutate.isPending ? <CircularProgress size={16} /> : undefined
            }
          >
            删除
          </Button>
        </DialogActions>
      </Dialog>

      <Snackbar
        open={snackbar.open}
        autoHideDuration={snackbar.severity === 'error' ? 8000 : 3000}
        onClose={() => setSnackbar({ ...snackbar, open: false })}
        anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
      >
        <Alert
          severity={snackbar.severity}
          onClose={() => setSnackbar({ ...snackbar, open: false })}
          sx={{ width: '100%' }}
        >
          {snackbar.message}
        </Alert>
      </Snackbar>
    </Card>
  );
}