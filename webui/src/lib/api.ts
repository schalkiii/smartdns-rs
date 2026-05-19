import {
  useQuery,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(path);
  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }
  return res.json();
}

async function apiPost<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }
  if (res.status === 201 || res.status === 204) {
    return undefined as T;
  }
  return res.json();
}

async function apiDelete<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return res.json();
}

export interface StatsSnapshot {
  timestamp: number;
  total_queries: number;
  cache_hits: number;
}

export interface StatsResponse {
  uptime_secs: number;
  active_queries: number;
  cache_size: number;
  cache_hits: number;
  cache_query_hits: number;
  total_queries: number;
  cache_hit_rate: number;
  avg_query_time_ms: number;
  version: string;
  history: StatsSnapshot[];
}

export interface SystemStatusResponse {
  server_name: string;
  version: string;
  build_date: string;
  uptime: number;
  config_loaded_at: string;
  active_queries: number;
}

export interface CacheEntry {
  name: string;
  hits: number;
  last_access: string;
  query_type: string;
  records: unknown[];
}

export interface CachesResponse {
  count: number;
  total: number;
  data: CacheEntry[];
}

export interface CacheConfigResponse {
  size: number;
  serve_expired: boolean;
  [key: string]: unknown;
}

export interface Nameserver {
  group: string[];
  server: string;
  [key: string]: unknown;
}

export interface NameserversResponse {
  count: number;
  data: Nameserver[];
}

export interface Listener {
  addr: string;
  port: number;
  protocol: string;
  [key: string]: unknown;
}

export interface ListenersResponse {
  count: number;
  data: Listener[];
}

export interface AddressEntry {
  domain: string;
  address: string;
  [key: string]: unknown;
}

export interface AddressesResponse {
  count: number;
  data: AddressEntry[];
}

export interface ForwardEntry {
  name: string;
  count: number;
  forwards: unknown[];
}

export interface ForwardsResponse {
  count: number;
  data: ForwardEntry[];
}

export interface ConfigResponse {
  server_name: string;
  conf_dir: string;
}

export interface VersionResponse {
  version: string;
}

const statsKeys = ['stats'] as const;
const systemStatusKeys = ['system-status'] as const;
const cachesKeys = ['caches'] as const;
const cacheConfigKeys = ['cache-config'] as const;
const nameserversKeys = ['nameservers'] as const;
const listenersKeys = ['listeners'] as const;
const addressesKeys = ['addresses'] as const;
const forwardsKeys = ['forwards'] as const;
const configKeys = ['config'] as const;
const versionKeys = ['version'] as const;

export function useStats() {
  return useQuery({
    queryKey: statsKeys,
    queryFn: () => apiGet<StatsResponse>('/api/stats'),
    refetchInterval: 5000,
  });
}

export function useSystemStatus() {
  return useQuery({
    queryKey: systemStatusKeys,
    queryFn: () => apiGet<SystemStatusResponse>('/api/system/status'),
  });
}

export function useCaches(offset = 0, limit = 50) {
  return useQuery({
    queryKey: [...cachesKeys, offset, limit],
    queryFn: () => apiGet<CachesResponse>(`/api/caches?offset=${offset}&limit=${limit}`),
    refetchInterval: 5000,
  });
}

export function useFlushCache() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => apiPost<void>('/api/caches/flush'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: cachesKeys });
    },
  });
}

export function useCacheConfig() {
  return useQuery({
    queryKey: cacheConfigKeys,
    queryFn: () => apiGet<CacheConfigResponse>('/api/caches/config'),
  });
}

export function useNameservers() {
  return useQuery({
    queryKey: nameserversKeys,
    queryFn: () => apiGet<NameserversResponse>('/api/nameservers'),
  });
}

export function useListeners() {
  return useQuery({
    queryKey: listenersKeys,
    queryFn: () => apiGet<ListenersResponse>('/api/listeners'),
  });
}

export function useAddresses() {
  return useQuery({
    queryKey: addressesKeys,
    queryFn: () => apiGet<AddressesResponse>('/api/addresses'),
  });
}

export function useCreateAddress() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (rule: { domain: string; address: string }) =>
      apiPost<void>('/api/addresses', { rule }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: addressesKeys });
    },
  });
}

export function useDeleteAddress() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (domain: string) =>
      apiDelete<void>('/api/addresses', { domain }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: addressesKeys });
    },
  });
}

export function useForwards() {
  return useQuery({
    queryKey: forwardsKeys,
    queryFn: () => apiGet<ForwardsResponse>('/api/forwards'),
  });
}

export function useConfig() {
  return useQuery({
    queryKey: configKeys,
    queryFn: () => apiGet<ConfigResponse>('/api/config'),
  });
}

export function useReloadConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => apiPost<void>('/api/config/reload'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: configKeys });
    },
  });
}

export function useVersion() {
  return useQuery({
    queryKey: versionKeys,
    queryFn: () => apiGet<VersionResponse>('/api/version'),
  });
}