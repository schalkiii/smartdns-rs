// 配置项目录：设置页控件类型、分组与悬浮说明的唯一事实源。
// 描述依据 src/config 各字段文档与 docs/port-gap-analysis.md §22。

export type OptionType = "bool" | "number" | "text" | "select";

export interface OptionChoice {
  value: string;
  label: string;
}

export interface OptionDef {
  key: string;
  label: string;
  description: string;
  type: OptionType;
  /** 未显式配置时的内置默认值（仅展示用） */
  default?: string;
  choices?: OptionChoice[];
  placeholder?: string;
  unit?: string;
  /** 附加警示（渲染在悬浮说明末尾） */
  warning?: string;
  /** 平台限制说明 */
  platform?: string;
  /** 解析器接受但运行时未接线 */
  unwired?: boolean;
}

export interface OptionGroup {
  id: string;
  title: string;
  description?: string;
  options: OptionDef[];
}

const bool = (
  key: string,
  label: string,
  description: string,
  extra?: Partial<OptionDef>
): OptionDef => ({ key, label, description, type: "bool", ...extra });

const num = (
  key: string,
  label: string,
  description: string,
  extra?: Partial<OptionDef>
): OptionDef => ({ key, label, description, type: "number", ...extra });

const text = (
  key: string,
  label: string,
  description: string,
  extra?: Partial<OptionDef>
): OptionDef => ({ key, label, description, type: "text", ...extra });

export const OPTION_GROUPS: OptionGroup[] = [
  {
    id: "general",
    title: "常规",
    description: "服务标识、工作目录与本地解析基础项。",
    options: [
      text("server-name", "服务器名称", "DNS 服务器名称，默认取本机主机名。", {
        placeholder: "默认：主机名",
      }),
      text("domain", "本地域名后缀", "附加到 DHCP 主机名与 hosts 条目的本地域名后缀。", {
        placeholder: "如 lan",
      }),
      text("user", "运行用户", "启动后切换到的运行用户（仅 Unix 生效）。", {
        platform: "仅 Unix",
        placeholder: "如 nobody",
      }),
      num("num-workers", "工作线程数", "异步运行时工作线程数量，默认为 CPU 核心数。一般无需修改。"),
      text("data-dir", "数据目录", "运行时数据（持久化缓存、自签证书等）的基准目录；相对路径按它解析。不配置则使用进程工作目录。", {
        placeholder: "如 ./data",
      }),
      text("resolv-file", "resolv.conf 路径", "从该文件读取上游 DNS 服务器作为补充。", {
        placeholder: "如 /etc/resolv.conf",
      }),
      bool("resolv-hostname", "解析本机主机名", "是否把本机主机名解析为本机地址。默认：配置了 hosts-file 时开启。"),
      text("hosts-file", "hosts 文件", "自定义 hosts 文件路径（支持 glob），提供本地域名→IP 映射。", {
        placeholder: "如 /etc/hosts",
      }),
      bool("expand-ptr-from-address", "从 address 规则展开 PTR", "为 address 规则中的 IP 自动生成反向解析（PTR）应答。默认关闭。"),
      bool("local-ptr-enable", "本地地址 PTR", "为本机地址回答 PTR 查询；关闭后回落上游。默认开启。"),
      text("local-domain", "局域网域名", "局域网域名后缀：hosts/租约中 <主机>.<后缀> 与裸主机名互相命中；该后缀查询还会路由到 mdns 上游组（需另开 mdns-lookup）。写 - 清除。", {
        placeholder: "如 lan",
      }),
      text("dnsmasq-lease-file", "dnsmasq 租约文件", "读取 dnsmasq DHCP 租约文件，解析局域网主机名与 PTR。文件变化自动重载。", {
        placeholder: "如 /var/lib/misc/dnsmasq.leases",
      }),
      text("odhcpd-lease-file", "odhcpd 租约文件", "读取 OpenWrt odhcpd 的 DHCPv6 租约文件，可与 dnsmasq 租约并存。", {
        placeholder: "如 /tmp/odhcp6c-leases",
      }),
      bool("mdns-lookup", "mDNS 查询", "把 local-domain 后缀的查询路由到 mDNS 上游组。默认关闭；只想用本地域名互命中时不要打开。"),
    ],
  },
  {
    id: "network",
    title: "监听与网络",
    description: "监听地址（bind*）在「上游服务器」页管理；此处为全局网络参数。",
    options: [
      num("tcp-idle-time", "TCP 空闲超时", "TCP 连接空闲超时时间。默认 120 秒。", {
        unit: "秒",
        default: "120",
      }),
      num("socket-buff-size", "套接字缓冲区", "监听 socket 的收发缓冲区大小（SO_RCVBUF/SO_SNDBUF），UDP 高 QPS 时防丢包。", {
        unit: "字节",
        placeholder: "如 524288",
        warning: "只接受纯整数字节：写 512K 这类后缀会被静默忽略。Linux 上受 net.core.rmem_max/wmem_max 上限约束。",
      }),
      num("max-query-limit", "最大并发查询", "并发查询上限，超出直接回 REFUSED，避免上游抖动时请求堆积。默认 65535。", {
        default: "65535",
        warning: "设太小会误伤并发下载资源页的场景；一般家用 4096 足够。",
      }),
      num("max-reply-ip-num", "最大应答 IP 数", "每个域名最多返回的 IP 数量（1~16）。默认 8。", {
        default: "8",
        placeholder: "1~16",
      }),
      text("edns-client-subnet", "EDNS Client Subnet", "向上游携带的客户端子网（ECS），影响 CDN 调度。", {
        placeholder: "如 192.168.1.1/24",
      }),
      text("dns64", "DNS64 前缀", "DNS64 合成前缀：为纯 IPv6 网络把 A 记录合成为 AAAA。", {
        placeholder: "如 64:ff9b::/96",
      }),
    ],
  },
  {
    id: "cache",
    title: "缓存",
    description: "缓存容量、持久化与过期策略。",
    options: [
      num("cache-size", "缓存条数上限", "缓存的最大条目数。默认 512。", {
        default: "512",
      }),
      num("cache-mem-size", "缓存内存上限", "缓存估算内存占用上限，超限按分片轮转淘汰 LRU；0 为不限。与 cache-size 是两条独立上限，谁先到谁生效。", {
        unit: "字节",
        default: "0（不限）",
        warning: "只接受纯整数字节。设太小会在条数没满时就淘汰，表现为命中率下降；内存充裕建议 0。",
      }),
      bool("cache-persist", "缓存持久化", "是否把缓存定期落盘并在启动时加载。默认关闭。"),
      text("cache-file", "缓存文件", "持久化缓存文件名。", {
        placeholder: "如 smartdns.cache",
        warning: "相对路径会再拼一次 data-dir：data-dir=./data 时这里只写文件名，写 ./data/xxx 会被拼成 data/data/xxx。",
      }),
      num("cache-checkpoint-time", "缓存落盘间隔", "持久化缓存的落盘周期。默认 86400 秒（24 小时）。", {
        unit: "秒",
        default: "86400",
      }),
      bool("prefetch-domain", "域名预取", "对热点域名在 TTL 到期前主动预取。默认关闭。"),
      bool("serve-expired", "过期缓存服务", "TTL 过期后仍先返回旧结果并后台刷新，降低等待。默认开启。"),
      num("serve-expired-ttl", "过期缓存保留时长", "过期缓存的最长保留时间；0 表示按内部策略。默认 0。", {
        unit: "秒",
        default: "0",
      }),
      num("serve-expired-reply-ttl", "过期应答 TTL", "返回过期缓存时给客户端的 TTL。默认 5 秒。", {
        unit: "秒",
        default: "5",
      }),
      num("serve-expired-prefetch-time", "过期预取窗口", "剩余 TTL 低于该值即后台刷新；0 时沿用内部默认（600 秒）。", {
        unit: "秒",
        default: "0",
      }),
    ],
  },
  {
    id: "ttl",
    title: "TTL",
    description: "资源记录 TTL 的钳制与本地记录 TTL。",
    options: [
      num("rr-ttl", "固定 TTL", "把所有资源记录的 TTL 固定为该值（覆盖上游 TTL）。", {
        unit: "秒",
      }),
      num("rr-ttl-min", "最小 TTL", "资源记录 TTL 下限，上游 TTL 低于它时抬升。", {
        unit: "秒",
      }),
      num("rr-ttl-max", "最大 TTL", "资源记录 TTL 上限。", {
        unit: "秒",
      }),
      num("rr-ttl-reply-max", "应答最大 TTL", "返回给客户端的 TTL 上限（不影响缓存中的 TTL）。", {
        unit: "秒",
      }),
      num("local-ttl", "本地记录 TTL", "address/hosts 等本地记录的 TTL。默认跟随 rr-ttl-min，未配置时为 10 秒。", {
        unit: "秒",
      }),
    ],
  },
  {
    id: "response",
    title: "应答与优选",
    description: "上游响应选择策略、测速与双栈优选。",
    options: [
      {
        key: "response-mode",
        label: "响应模式",
        description: "多上游并发查询时的应答选择策略。first-ping：按 ping 最快；fastest-ip：按测速最快 IP；fastest-response：返回首个 NoError 响应。默认 first-ping。",
        type: "select",
        default: "first-ping",
        choices: [
          { value: "first-ping", label: "first-ping（ping 最快）" },
          { value: "fastest-ip", label: "fastest-ip（测速最快 IP）" },
          { value: "fastest-response", label: "fastest-response（首个响应）" },
        ],
        warning: "speed-check-mode none 时测速类模式无意义，建议 fastest-response。别名 response 等价。",
      },
      text("speed-check-mode", "测速模式", "IP 测速方式列表，逗号分隔：none / ping / tcp:端口 / http:端口 / https:端口。", {
        placeholder: "如 none 或 ping,tcp:80,tcp:443",
        warning: "测速会消耗连接与 CPU；高 QPS 或资源受限环境建议 none。",
      }),
      bool("dualstack-ip-selection", "双栈 IP 优选", "IPv4/IPv6 双栈优选策略。默认开启。", {
        warning: "依赖测速：speed-check-mode none 时选不中、纯拖时间，建议显式关闭。",
      }),
      num("dualstack-ip-selection-threshold", "双栈优选阈值", "双栈优选的延迟差阈值（0~1000）。默认 10。", {
        unit: "毫秒",
        default: "10",
      }),
      bool("dualstack-ip-allow-force-AAAA", "允许强制 AAAA", "双栈优选时允许强制返回 AAAA。默认关闭。"),
      bool("force-AAAA-SOA", "强制 AAAA 返回 SOA", "把 AAAA 查询强制改为 SOA 应答。默认关闭。", {
        warning: "会破坏 AAAA 真实记录与否定响应的缓存，仅在确认不需要 IPv6 时使用。",
      }),
      bool("force-HTTPS-SOA", "强制 HTTPS 返回 SOA", "把 HTTPS（SVCB/HTTPS 记录）查询强制改为 SOA 应答。默认关闭。"),
      bool("force-no-CNAME", "裁剪 CNAME", "应答中裁掉 CNAME 只保留地址记录（CNAME 类型查询本身不裁）。默认关闭。", {
        warning: "会破坏依赖 CNAME 链的客户端（部分 CDN 健康检查、dnsmasq 转发校验）。",
      }),
    ],
  },
  {
    id: "log",
    title: "日志",
    description: "运行日志（tracing）输出与轮转。",
    options: [
      {
        key: "log-level",
        label: "日志级别",
        description: "运行日志级别。notice 等价 info，fatal 等价 error。默认 info。",
        type: "select",
        default: "info",
        choices: [
          { value: "trace", label: "trace" },
          { value: "debug", label: "debug" },
          { value: "info", label: "info" },
          { value: "warn", label: "warn" },
          { value: "error", label: "error" },
        ],
      },
      text("log-file", "日志文件", "日志文件路径；不配置则只输出到控制台。", {
        placeholder: "如 ./logs/smartdns.log",
      }),
      text("log-size", "单文件大小", "单个日志文件大小上限，支持 K/M/G 后缀。默认 128K。", {
        default: "128K",
        placeholder: "如 16M",
      }),
      num("log-num", "轮转份数", "日志轮转保留的文件份数。默认 2。", {
        default: "2",
      }),
      text("log-file-mode", "文件权限", "日志文件权限（八进制）。默认 0640。", {
        default: "0640",
        placeholder: "如 0644",
        platform: "仅 Unix",
      }),
      bool("log-console", "控制台日志", "是否同时输出日志到控制台。"),
      bool("log-color", "日志着色", "日志 ANSI 着色。默认开启；写入文件或 journald 时建议关闭。"),
      text("log-filter", "日志过滤", "按关键字过滤日志输出。", {
        placeholder: "过滤关键字",
      }),
      bool("log-syslog", "转发 syslog", "运行日志额外转发 syslog。", {
        platform: "仅 Unix",
        warning: "Windows/macOS 无 syslog 后端，启动时打印不支持告警。",
      }),
    ],
  },
  {
    id: "audit",
    title: "审计",
    description: "逐查询审计记录（隐私敏感，默认关闭）。WebUI 查询日志走内存环形缓冲，不受此处开关影响。",
    options: [
      bool("audit-enable", "启用审计", "记录每条 DNS 查询到审计日志。默认关闭。", {
        warning: "每条查询都有额外开销；把查询域名落盘等同记录隐私，按需开启。",
      }),
      text("audit-file", "审计文件", "审计日志文件路径。", {
        placeholder: "如 ./logs/audit.log",
      }),
      text("audit-size", "单文件大小", "单个审计文件大小上限，支持 K/M/G 后缀。", {
        placeholder: "如 4M",
      }),
      num("audit-num", "轮转份数", "审计日志轮转保留份数。"),
      text("audit-file-mode", "文件权限", "审计文件权限（八进制）。", {
        placeholder: "如 0640",
        platform: "仅 Unix",
      }),
      bool("audit-SOA", "审计 SOA 应答", "是否审计只有 SOA、无地址记录的应答。默认关闭（对齐 C 版，减少噪声）。"),
      bool("audit-console", "审计到控制台", "审计同时打到控制台。", {
        warning: "systemd/容器场景等于把查询隐私写进日志采集面，只在临时抓现场时开。",
      }),
      bool("audit-syslog", "审计到 syslog", "审计改发 syslog（需同时开启 audit-enable）。", {
        platform: "仅 Unix",
      }),
    ],
  },
  {
    id: "cert",
    title: "证书与加密",
    description: "DoT/DoH/DoQ 监听证书与上游 TLS 校验。加密监听（bind-tls 等）在「上游服务器」页管理。",
    options: [
      text("bind-cert-file", "证书文件", "监听用 SSL 证书文件路径（PEM）。", {
        placeholder: "如 /path/server.crt",
      }),
      text("bind-cert-key-file", "证书私钥", "监听用 SSL 私钥文件路径（PEM）。", {
        placeholder: "如 /path/server.key",
      }),
      text("bind-cert-key-pass", "私钥密码", "私钥文件密码。", {
        unwired: true,
        warning: "当前版本只解析不生效：加密私钥请先自行解密成明文 PEM 再配置 bind-cert-key-file。",
      }),
      bool("bind-cert-generate", "自动生成自签证书", "未显式配置证书时自动生成/复用自签证书（落在 data-dir，未配置则临时目录）。默认关闭。", {
        warning: "自签只适合内网/测试；客户端仍需信任或跳过校验。",
      }),
      text("bind-cert-root-key-file", "自签 CA 私钥", "用该 PEM 私钥作为 CA 签发生成的证书，否则纯自签。", {
        placeholder: "如 /path/ca-key.pem",
      }),
      text("bind-cert-san", "自签证书 SAN", "自签证书的附加 SAN 列表，逗号分隔（域名或 IP）。", {
        placeholder: "如 dns.example.com,192.0.2.10",
      }),
      num("bind-cert-validity-days", "自签证书有效期", "自签证书有效天数。", {
        unit: "天",
        placeholder: "如 30",
      }),
      text("ca-file", "CA 证书文件", "校验上游 TLS 证书用的 CA 文件。", {
        placeholder: "如 /etc/ssl/certs/ca.pem",
      }),
      text("ca-path", "CA 证书目录", "校验上游 TLS 证书用的 CA 目录。", {
        placeholder: "如 /etc/ssl/certs",
      }),
    ],
  },
  {
    id: "acl",
    title: "访问控制",
    description: "客户端白名单。client-rules / ip-rules 明细在本页「高级结构项」编辑。",
    options: [
      bool("acl-enable", "启用 ACL 白名单", "开启后未匹配任何 client-rules 的客户端返回 REFUSED（后台预取豁免）。默认关闭。", {
        warning: "开启前务必先把所有合法客户端段写全，否则开启瞬间会拒掉整个网段。",
      }),
    ],
  },
  {
    id: "firewall",
    title: "防火墙下发（Linux）",
    description: "ipset/nftset 规则明细在本页「高级结构项」编辑；此处为全局开关。",
    options: [
      bool("ipset-timeout", "ipset 条目超时", "ipset 条目超时 = 应答 TTL × 3；关闭则永久驻留。默认关闭。", {
        platform: "仅 Linux",
      }),
      text("ipset-no-speed", "ipset 测速失败集", "测速不可达的 IP 写入该 ipset（需预先创建）。", {
        platform: "仅 Linux",
        placeholder: "set 名称",
      }),
      bool("nftset-timeout", "nftset 元素超时", "nftset 元素超时 = 应答 TTL × 3；关闭则永久驻留。默认关闭。", {
        platform: "仅 Linux",
      }),
      bool("nftset-debug", "nftset 调试日志", "逐条打印下发的 IP、set 名与超时。只在排查下发失败时开。", {
        platform: "仅 Linux",
      }),
      text("nftset-no-speed", "nftset 测速失败集", "测速不可达的 IP 写入的 nft set 规格。", {
        platform: "仅 Linux",
        placeholder: "如 #4:inet#filter#nospeed",
      }),
    ],
  },
  {
    id: "debug",
    title: "调试",
    description: "排障专用，不要长期开启。",
    options: [
      bool("debug-save-fail-packet", "保存失败报文", "把失败请求的原始报文写入磁盘。默认关闭。", {
        warning: "长期开启会持续产生小文件、无自动清理，且文件名含查询域名（等同把查询记录落盘）。",
      }),
      text("debug-save-fail-packet-dir", "失败报文目录", "失败报文保存目录，默认 /tmp/smartdns（Windows 上落到当前盘符根目录，请显式指定）。", {
        placeholder: "如 ./data/fail-packets",
      }),
    ],
  },
];

// ---------------------------------------------------------------------------
// 结构项家族（多实例键）元数据：供通用行编辑器使用
// ---------------------------------------------------------------------------

export interface FamilyDef {
  family: string;
  title: string;
  description: string;
  /** 家族内可选键（多键家族用下拉选择） */
  keys: OptionChoice[];
  valuePlaceholder: string;
  valueHelp: string;
}

export const FAMILY_DEFS: Record<string, FamilyDef> = {
  server: {
    family: "server",
    title: "上游 DNS 服务器",
    description:
      "server / server-udp / server-tcp / server-tls / server-https / server-quic / server-h3。可附 -group 分组、-exclude-default-group 等参数；加密上游建议显式写 scheme。",
    keys: [
      { value: "server", label: "server（UDP）" },
      { value: "server-udp", label: "server-udp" },
      { value: "server-tcp", label: "server-tcp" },
      { value: "server-tls", label: "server-tls（DoT）" },
      { value: "server-https", label: "server-https（DoH）" },
      { value: "server-quic", label: "server-quic（DoQ）" },
      { value: "server-h3", label: "server-h3（DoH3）" },
      { value: "server-http3", label: "server-http3（DoH3 别名）" },
    ],
    valuePlaceholder: "如 223.5.5.5 或 https://doh.pub/dns-query -group cn",
    valueHelp:
      "地址可带端口（IP:port）。server-h3/http3 只在没写 scheme 时补 h3://，写 https:// 会静默变成 DoH。",
  },
  bind: {
    family: "bind",
    title: "监听地址",
    description:
      "bind（UDP）/ bind-tcp / bind-tls / bind-https / bind-http / bind-quic / bind-h3。bind-http 是 WebUI/API 的 HTTP 监听。加密监听可附 -server-name 与证书参数；未配证书且开启 bind-cert-generate 时自动自签。",
    keys: [
      { value: "bind", label: "bind（UDP）" },
      { value: "bind-udp", label: "bind-udp" },
      { value: "bind-tcp", label: "bind-tcp" },
      { value: "bind-tls", label: "bind-tls（DoT）" },
      { value: "bind-https", label: "bind-https（DoH）" },
      { value: "bind-http", label: "bind-http（HTTP API）" },
      { value: "bind-quic", label: "bind-quic（DoQ）" },
      { value: "bind-h3", label: "bind-h3（DoH3）" },
    ],
    valuePlaceholder: "如 [::]:9053 或 [::]:853 -server-name dns.example.com",
    valueHelp: "格式 [地址]:端口；省略地址监听全部网卡。修改监听需重启服务才完全生效。",
  },
  forward: {
    family: "forward",
    title: "域名转发规则",
    description: "nameserver /域名/组名或上游：把指定域名（含子域）路由到指定上游组。",
    keys: [{ value: "nameserver", label: "nameserver" }],
    valuePlaceholder: "如 /example.com/office 或 /google.com/https://doh.pub/dns-query",
    valueHelp: "域名两侧必须带 /；目标是上游组名或完整上游地址。",
  },
  cname: {
    family: "cname",
    title: "CNAME 规则",
    description: "cname /域名/目标域名：为指定域名设置 CNAME 别名；写 - 忽略该域名的 CNAME。",
    keys: [{ value: "cname", label: "cname" }],
    valuePlaceholder: "如 /www.example.com/example.com",
    valueHelp: "域名两侧必须带 /。",
  },
  "txt-record": {
    family: "txt-record",
    title: "TXT 静态记录",
    description: "txt-record /域名/文本：同域名多行累积为同一个 TXT RRset。",
    keys: [{ value: "txt-record", label: "txt-record" }],
    valuePlaceholder: '如 /example.com/v=spf1 -all',
    valueHelp: "文本含空格时可用引号包裹。",
  },
  "srv-record": {
    family: "srv-record",
    title: "SRV 静态记录",
    description: "srv-record /域名/目标,端口,优先级,权重：多值累积。",
    keys: [{ value: "srv-record", label: "srv-record" }],
    valuePlaceholder: "如 /_xmpp-client._tcp/xmpp.example.com,5222,0,5",
    valueHelp: "顺序是 目标,端口,优先级,权重 —— 与 C 版的 priority weight port target 不同。",
  },
  "https-record": {
    family: "https-record",
    title: "HTTPS 静态记录",
    description: "https-record /域名/…：为域名配置 HTTPS（SVCB）记录或过滤/屏蔽策略。",
    keys: [{ value: "https-record", label: "https-record" }],
    valuePlaceholder: "如 /example.com/# 或 /example.com/-",
    valueHelp: "# 返回 SOA 屏蔽；- 忽略；也可写完整 HTTPS 记录数据。",
  },
  "domain-rule": {
    family: "domain-rule",
    title: "域名规则",
    description:
      "domain-rule /域名/ -参数：按域名覆盖测速、双栈、TTL、上游组等行为（如 -speed-check-mode none -dualstack-ip-selection no）。",
    keys: [
      { value: "domain-rule", label: "domain-rule" },
      { value: "domain-rules", label: "domain-rules（别名）" },
    ],
    valuePlaceholder: "如 /example.com/-speed-check-mode none -rr-ttl-min 60",
    valueHelp: "域名两侧必须带 /；后跟一个或多个 -参数。",
  },
  "client-rule": {
    family: "client-rule",
    title: "客户端规则",
    description:
      "client-rules <IP/CIDR> [-group 组名]：把客户端网段划入上游组；配合 acl-enable 构成白名单。",
    keys: [
      { value: "client-rules", label: "client-rules" },
      { value: "client-rule", label: "client-rule（别名）" },
    ],
    valuePlaceholder: "如 192.168.1.0/24 -group office",
    valueHelp: "开启 acl-enable 前务必把所有合法客户端段写全。",
  },
  "ip-rules": {
    family: "ip-rules",
    title: "客户端 IP 过滤规则",
    description:
      "ip-rules <CIDR> [-blacklist-ip] [-whitelist-ip] [-bogus-nxdomain] [-ignore-ip]：按请求源网段附加 IP 过滤集合。",
    keys: [{ value: "ip-rules", label: "ip-rules" }],
    valuePlaceholder: "如 192.168.1.0/24 -blacklist-ip",
    valueHelp: "无匹配时复用全局集合。",
  },
  "group-match": {
    family: "group-match",
    title: "分组匹配（语法糖）",
    description:
      "group-match -g 组名 -d 域名 -c 客户端段：等价于 nameserver /域名/组 + client-rules <段> -group 组。",
    keys: [{ value: "group-match", label: "group-match" }],
    valuePlaceholder: "如 -g office -d example.com -c 192.168.1.0/24",
    valueHelp: "-d / -c 可多次出现。",
  },
  group: {
    family: "group",
    title: "上游组块",
    description:
      "group-begin 组名 … group-end：块内的 server/nameserver 等配置属于该组。必须成对出现，块内可嵌套其它结构项。",
    keys: [
      { value: "group-begin", label: "group-begin" },
      { value: "group-end", label: "group-end（无值）" },
    ],
    valuePlaceholder: "组名（group-end 留空）",
    valueHelp: "行顺序即配置顺序：先 group-begin，再组内条目，最后 group-end。",
  },
  "blacklist-ip": {
    family: "blacklist-ip",
    title: "IP 黑名单",
    description: "blacklist-ip <IP/CIDR 或 ip-set:名称>：上游配置 -blacklist-ip 时过滤这些地址。",
    keys: [{ value: "blacklist-ip", label: "blacklist-ip" }],
    valuePlaceholder: "如 243.185.187.39/32 或 ip-set:ads",
    valueHelp: "可写网段或引用 ip-set。",
  },
  "whitelist-ip": {
    family: "whitelist-ip",
    title: "IP 白名单",
    description: "whitelist-ip <IP/CIDR 或 ip-set:名称>：上游配置 -whitelist-ip 时只接受这些地址。",
    keys: [{ value: "whitelist-ip", label: "whitelist-ip" }],
    valuePlaceholder: "如 192.168.0.0/16",
    valueHelp: "可写网段或引用 ip-set。",
  },
  "bogus-nxdomain": {
    family: "bogus-nxdomain",
    title: "Bogus NXDOMAIN",
    description: "bogus-nxdomain <IP/CIDR>：把解析到这些地址的 NXDOMAIN 视为污染并丢弃。",
    keys: [{ value: "bogus-nxdomain", label: "bogus-nxdomain" }],
    valuePlaceholder: "如 243.185.187.39",
    valueHelp: "常用于运营商 DNS 劫持地址。",
  },
  "ignore-ip": {
    family: "ignore-ip",
    title: "忽略 IP",
    description: "ignore-ip <IP/CIDR>：应答中忽略这些地址。",
    keys: [{ value: "ignore-ip", label: "ignore-ip" }],
    valuePlaceholder: "如 10.0.0.0/8",
    valueHelp: "可写网段或引用 ip-set。",
  },
  "ip-set": {
    family: "ip-set",
    title: "IP 集合",
    description: "ip-set -name 名称 -url/-file 来源：定义可被 blacklist-ip 等引用的 IP 集合。",
    keys: [{ value: "ip-set", label: "ip-set" }],
    valuePlaceholder: "如 -name ads -url https://example.com/ips.txt",
    valueHelp: "来源支持 http(s) URL 或本地文件。",
  },
  "ip-alias": {
    family: "ip-alias",
    title: "IP 别名",
    description: "ip-alias <原IP/CIDR> <目标IP列表>：把应答中的地址替换为别名地址。",
    keys: [{ value: "ip-alias", label: "ip-alias" }],
    valuePlaceholder: "如 1.2.3.4 5.6.7.8",
    valueHelp: "目标可写多个 IP。",
  },
  ipset: {
    family: "ipset",
    title: "ipset 下发规则",
    description: "ipset /域名/set名：把该域名应答的 IP 写入 ipset（set 须预先存在，v4/v6 分开写）。",
    keys: [{ value: "ipset", label: "ipset" }],
    valuePlaceholder: "如 /example.com/dns_v4",
    valueHelp: "仅 Linux；调用 ipset CLI。",
  },
  nftset: {
    family: "nftset",
    title: "nftset 下发规则",
    description:
      "nftset /域名/#4:family#table#set,#6:…：把应答 IP 写入 nftables set。family 取 ip/ip6/inet。",
    keys: [{ value: "nftset", label: "nftset" }],
    valuePlaceholder: "如 /example.com/#4:ip#filter#dns_v4,#6:ip6#filter#dns_v6",
    valueHelp: "仅 Linux（需 nft 特性，默认已含）。",
  },
  "proxy-server": {
    family: "proxy-server",
    title: "代理服务器",
    description:
      "proxy-server <URL> -name 名称：定义 HTTP/SOCKS5 代理，供上游以 -proxy 引用（如 DoH 走本地代理）。",
    keys: [{ value: "proxy-server", label: "proxy-server" }],
    valuePlaceholder: "如 socks5://127.0.0.1:1080 -name local",
    valueHelp: "支持 http:// 与 socks5://。",
  },
  "conf-file": {
    family: "conf-file",
    title: "包含配置文件",
    description:
      "conf-file <路径>：加载额外配置文件（按出现顺序内联展开）。注意：include 中的同名项会覆盖其前面的主配置项。",
    keys: [{ value: "conf-file", label: "conf-file" }],
    valuePlaceholder: "如 rules/anti-ad.conf",
    valueHelp: "相对路径相对主配置目录解析。",
  },
  "force-qtype-soa": {
    family: "force-qtype-soa",
    title: "强制 QTYPE 返回 SOA",
    description:
      "force-qtype-soa <类型ID…>：指定查询类型强制返回 SOA（如 65=HTTPS、28=AAAA）。多行累积生效。",
    keys: [{ value: "force-qtype-soa", label: "force-qtype-soa" }],
    valuePlaceholder: "如 65 28",
    valueHelp: "类型 ID 见 DNS 记录类型列表；多行会累积而非覆盖。",
  },
};

/** 设置页「高级结构项」展示的家族（其余家族嵌入上游/规则页） */
export const ADVANCED_FAMILIES = [
  "domain-rule",
  "client-rule",
  "ip-rules",
  "group-match",
  "group",
  "blacklist-ip",
  "whitelist-ip",
  "bogus-nxdomain",
  "ignore-ip",
  "ip-set",
  "ip-alias",
  "ipset",
  "nftset",
  "conf-file",
  "force-qtype-soa",
] as const;