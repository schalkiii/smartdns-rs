'use client';

// 配置设置页：覆盖主配置文件的全部单值选项（分组 + 悬浮说明），
// 多实例结构项在「高级结构项」区以通用行编辑器管理，
// server/bind/address/domain-set 等则嵌入上游/规则/广告拦截页。

import { useMemo, useState } from 'react';
import Box from '@mui/material/Box';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import Paper from '@mui/material/Paper';
import Typography from '@mui/material/Typography';
import TextField from '@mui/material/TextField';
import Switch from '@mui/material/Switch';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import Chip from '@mui/material/Chip';
import Alert from '@mui/material/Alert';
import Skeleton from '@mui/material/Skeleton';
import Snackbar from '@mui/material/Snackbar';
import CircularProgress from '@mui/material/CircularProgress';
import Accordion from '@mui/material/Accordion';
import AccordionSummary from '@mui/material/AccordionSummary';
import AccordionDetails from '@mui/material/AccordionDetails';
import FormControl from '@mui/material/FormControl';
import Select from '@mui/material/Select';
import MenuItem from '@mui/material/MenuItem';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import ExpandMoreIcon from '@mui/icons-material/ExpandMore';
import RestartAltIcon from '@mui/icons-material/RestartAlt';
import SaveIcon from '@mui/icons-material/Save';
import {
  useConfigOptions,
  useUpdateConfigOptions,
} from '@/lib/api';
import {
  OPTION_GROUPS,
  ADVANCED_FAMILIES,
  type OptionDef,
} from '@/lib/config-catalog';
import ConfigLinesEditor from '../components/ConfigLinesEditor';
import { useDashboardTab } from '../dashboard-context';

interface SnackbarState {
  open: boolean;
  message: string;
  severity: 'success' | 'error';
}

/** 草稿值：string=新值，null=注释掉该行（回落默认）；缺省=未改动 */
type Draft = Record<string, string | null>;

function optionTooltip(option: OptionDef) {
  return (
    <Box sx={{ maxWidth: 360 }}>
      <Typography variant="body2">{option.description}</Typography>
      {option.default !== undefined && (
        <Typography variant="body2" sx={{ mt: 0.5 }}>
          内置默认：{option.default}
        </Typography>
      )}
      {option.platform && (
        <Typography variant="body2" sx={{ mt: 0.5 }}>
          平台：{option.platform}
        </Typography>
      )}
      {option.warning && (
        <Typography variant="body2" color="warning.light" sx={{ mt: 0.5 }}>
          ⚠ {option.warning}
        </Typography>
      )}
      <Typography variant="caption" color="text.secondary" sx={{ mt: 0.5, display: 'block' }}>
        配置键：{option.key}
      </Typography>
    </Box>
  );
}

function OptionRow({
  option,
  serverValue,
  draftValue,
  hasDraft,
  onChange,
  onReset,
}: {
  option: OptionDef;
  serverValue: string | undefined;
  draftValue: string | null | undefined;
  hasDraft: boolean;
  onChange: (value: string) => void;
  onReset: () => void;
}) {
  const effective = hasDraft ? draftValue : (serverValue ?? null);
  const isUnset = effective === null || effective === undefined;
  const dirty =
    hasDraft && draftValue !== (serverValue === undefined ? null : serverValue);

  let control: React.ReactNode;
  if (option.type === 'bool') {
    const checked = isUnset
      ? option.default === 'yes'
      : effective === 'yes' || effective === 'true';
    control = (
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
        <Switch
          checked={checked}
          onChange={(e) => onChange(e.target.checked ? 'yes' : 'no')}
        />
        <Typography variant="body2" color="text.secondary">
          {isUnset ? `未配置（默认${checked ? '开启' : '关闭'}）` : checked ? '开启' : '关闭'}
        </Typography>
      </Box>
    );
  } else if (option.type === 'select') {
    control = (
      <FormControl size="small" sx={{ minWidth: 260 }}>
        <Select
          value={isUnset ? '' : effective}
          displayEmpty
          onChange={(e) => onChange(e.target.value)}
          renderValue={(v) =>
            v === '' ? `未配置（默认 ${option.default ?? '-'}）` : String(v)
          }
        >
          {option.choices?.map((c) => (
            <MenuItem key={c.value} value={c.value}>
              {c.label}
            </MenuItem>
          ))}
        </Select>
      </FormControl>
    );
  } else {
    control = (
      <TextField
        size="small"
        type={option.type === 'number' ? 'number' : 'text'}
        value={isUnset ? '' : effective}
        onChange={(e) => onChange(e.target.value)}
        placeholder={
          option.placeholder ??
          (option.default !== undefined ? `默认 ${option.default}` : '未配置')
        }
        helperText={
          option.unit ? `单位：${option.unit}` : undefined
        }
        sx={{ width: { xs: '100%', sm: 320 } }}
        slotProps={{ htmlInput: { style: { fontFamily: 'monospace' } } }}
      />
    );
  }

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: { xs: 'column', sm: 'row' },
        alignItems: { sm: 'center' },
        gap: { xs: 0.5, sm: 2 },
        py: 1.25,
        borderBottom: 1,
        borderColor: 'divider',
        '&:last-child': { borderBottom: 0 },
        opacity: option.unwired ? 0.6 : 1,
      }}
    >
      <Box
        sx={{
          width: { sm: 280 },
          flexShrink: 0,
          display: 'flex',
          alignItems: 'center',
          gap: 0.5,
        }}
      >
        <Typography variant="body2" sx={{ fontWeight: 500 }}>
          {option.label}
        </Typography>
        <Tooltip title={optionTooltip(option)} arrow placement="top">
          <InfoOutlinedIcon
            fontSize="small"
            color={option.warning ? 'warning' : 'action'}
            sx={{ cursor: 'help' }}
            tabIndex={0}
            role="img"
            aria-label={`${option.label} 配置说明`}
          />
        </Tooltip>
        {option.platform && (
          <Chip label={option.platform} size="small" variant="outlined" />
        )}
        {option.unwired && (
          <Chip label="未生效" size="small" color="warning" variant="outlined" />
        )}
        {dirty && <Chip label="已修改" size="small" color="primary" />}
      </Box>
      <Box sx={{ flexGrow: 1, display: 'flex', alignItems: 'center', gap: 1 }}>
        {control}
        {(serverValue !== undefined || hasDraft) && (
          <Tooltip title="恢复默认：注释掉配置文件中的该行">
            <IconButton size="small" onClick={onReset}>
              <RestartAltIcon fontSize="small" />
            </IconButton>
          </Tooltip>
        )}
      </Box>
    </Box>
  );
}

export default function SettingsTab() {
  const { data, isLoading, error } = useConfigOptions();
  const update = useUpdateConfigOptions();
  const { setCurrentTab } = useDashboardTab();
  const [draft, setDraft] = useState<Draft>({});
  const [snackbar, setSnackbar] = useState<SnackbarState>({
    open: false,
    message: '',
    severity: 'success',
  });

  const serverValues = data?.values ?? {};

  const dirtyKeys = useMemo(
    () =>
      Object.keys(draft).filter(
        (key) => draft[key] !== (serverValues[key] === undefined ? null : serverValues[key])
      ),
    [draft, serverValues]
  );

  function setOption(key: string, value: string) {
    setDraft((d) => ({ ...d, [key]: value }));
  }

  function resetOption(key: string) {
    setDraft((d) => ({ ...d, [key]: null }));
  }

  function save() {
    const options: Record<string, string | null> = {};
    for (const key of dirtyKeys) {
      const value = draft[key];
      if (typeof value === 'string' && value.trim() === '') {
        setSnackbar({
          open: true,
          message: `「${key}」的值为空：请填写值，或点恢复默认按钮移除该项`,
          severity: 'error',
        });
        return;
      }
      options[key] = typeof value === 'string' ? value.trim() : null;
    }
    update.mutate(options, {
      onSuccess: () => {
        setDraft({});
        setSnackbar({
          open: true,
          message: `已保存 ${Object.keys(options).length} 项并热更新配置`,
          severity: 'success',
        });
      },
      onError: (e: Error) => {
        setSnackbar({ open: true, message: e.message, severity: 'error' });
      },
    });
  }

  return (
    <Box>
      <Typography variant="h4" sx={{ mb: 1, fontWeight: 700 }}>
        配置设置
      </Typography>
      <Typography variant="body2" color="text.secondary" sx={{ mb: 3 }}>
        编辑主配置文件
        {data?.conf_file ? `（${data.conf_file}）` : ''}
        中的全部单值选项；保存后自动热更新。鼠标悬停 ⓘ 查看配置说明。
        上游服务器与监听地址在「上游服务器」页管理，地址规则在「规则管理」页，
        拦截订阅在「广告拦截」页。
      </Typography>

      {isLoading ? (
        <Box>
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} variant="rounded" height={64} sx={{ mb: 1 }} />
          ))}
        </Box>
      ) : error ? (
        <Alert severity="error">加载配置失败：{String(error)}</Alert>
      ) : (
        <>
          {/* 保存栏 */}
          <Paper
            elevation={0}
            sx={{
              position: 'sticky',
              top: 72,
              zIndex: 10,
              p: 1.5,
              mb: 3,
              display: 'flex',
              alignItems: 'center',
              gap: 2,
              border: 1,
              borderColor: dirtyKeys.length ? 'primary.main' : 'divider',
              borderRadius: 2,
              bgcolor: 'background.paper',
            }}
          >
            <Typography variant="body2" sx={{ flexGrow: 1 }}>
              {dirtyKeys.length
                ? `${dirtyKeys.length} 项待保存`
                : '没有未保存的更改'}
            </Typography>
            <Button
              size="small"
              disabled={!dirtyKeys.length || update.isPending}
              onClick={() => setDraft({})}
            >
              放弃更改
            </Button>
            <Button
              size="small"
              variant="contained"
              startIcon={
                update.isPending ? (
                  <CircularProgress size={16} />
                ) : (
                  <SaveIcon />
                )
              }
              disabled={!dirtyKeys.length || update.isPending}
              onClick={save}
            >
              保存并热更新
            </Button>
          </Paper>

          {OPTION_GROUPS.map((group) => (
            <Card key={group.id} variant="outlined" sx={{ mb: 3 }}>
              <CardContent>
                <Typography variant="h6" sx={{ fontWeight: 600 }}>
                  {group.title}
                </Typography>
                {group.description && (
                  <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
                    {group.description}
                  </Typography>
                )}
                <Box>
                  {group.options.map((option) => (
                    <OptionRow
                      key={option.key}
                      option={option}
                      serverValue={serverValues[option.key]}
                      draftValue={draft[option.key]}
                      hasDraft={option.key in draft}
                      onChange={(v) => setOption(option.key, v)}
                      onReset={() => resetOption(option.key)}
                    />
                  ))}
                </Box>
              </CardContent>
            </Card>
          ))}

          {/* 高级结构项 */}
          <Accordion variant="outlined" sx={{ mb: 3 }}>
            <AccordionSummary expandIcon={<ExpandMoreIcon />}>
              <Box>
                <Typography variant="h6" sx={{ fontWeight: 600 }}>
                  高级结构项
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  域名规则 / 客户端规则 / IP 过滤 / 分组 / 防火墙下发 / 包含文件等多行配置
                </Typography>
              </Box>
            </AccordionSummary>
            <AccordionDetails>
              <Alert severity="info" sx={{ mb: 2 }}>
                此处直接编辑主配置文件中的原始行，保存前会用解析器校验语法。
                地址规则（address）请在「规则管理」页编辑，拦截订阅（domain-set）请在「广告拦截」页编辑。
              </Alert>
              {ADVANCED_FAMILIES.map((family) => (
                <ConfigLinesEditor key={family} family={family} />
              ))}
            </AccordionDetails>
          </Accordion>

          {/* 结构项入口指引 */}
          <Card variant="outlined">
            <CardContent>
              <Typography variant="h6" sx={{ fontWeight: 600, mb: 1 }}>
                其他配置入口
              </Typography>
              <Box sx={{ display: 'flex', gap: 1, flexWrap: 'wrap' }}>
                <Button variant="outlined" size="small" onClick={() => setCurrentTab('upstream')}>
                  上游服务器 / 监听地址 / 代理
                </Button>
                <Button variant="outlined" size="small" onClick={() => setCurrentTab('rules')}>
                  地址规则 / 转发 / 静态记录
                </Button>
                <Button variant="outlined" size="small" onClick={() => setCurrentTab('adblock')}>
                  广告拦截订阅
                </Button>
              </Box>
            </CardContent>
          </Card>
        </>
      )}

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
    </Box>
  );
}