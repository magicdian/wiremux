# Host CLI

Host 侧首期使用 Rust 实现，目标是单文件可执行程序。

当前命令形态：

```bash
wiremux listen --port /dev/tty.usbmodem2101 --baud 115200
wiremux send --port /dev/tty.usbmodem2101 --channel 1 --line help
wiremux listen --port /dev/tty.usbmodem2101 --channel 1 --line help
wiremux passthrough --port /dev/tty.usbmodem2101 --channel 1
wiremux tui --port /dev/tty.usbmodem2101 --baud 115200 --tui-fps 120
```

当前能力：

- 使用 `serialport` backend 打开指定设备路径，并配置波特率。
- 支持 host 全局配置文件保存默认物理串口 profile：串口路径、波特率、数据位、
  停止位、校验位和流控。
- 从 mixed stream 中扫描 `WMUX` magic。
- 校验 version、length、CRC32。
- C core 侧也提供同等的单帧 decode/validate API，ESP 入站路径复用该公共规则。
- 默认终端输出保持简洁：有 `--channel` 时只显示该 channel 的原始 payload；无
  `--channel` 时普通终端字节原样保留，mux record 以 `chN> ` 或 manifest channel
  name 可用时的 `chN(name)> ` 标识来源。
- 完整 mux frame 诊断写入系统临时目录下的 `wiremux` 日志文件；启动时 host 会打印
  一行 `wiremux> diagnostics: <path>` 指出文件位置。
- 诊断日志包含 frame metadata 和 `payload_type`；manifest 会以
  `wiremux.v1.DeviceManifest` 标识。
- batch frame 会以 `wiremux.v1.MuxBatch` 标识；host 会根据 compression metadata
  解压并逐条显示其中的 channel record，batch summary 和完整 record metadata 写入
  diagnostics 日志。
- 非 mux 字节按普通终端输出保留。
- 构造 host-to-device input `MuxEnvelope`，并通过同一个 `WMUX` frame 格式发送到指定 channel。
- `wiremux tui` 提供 ratatui 交互界面，用同一个串口 handle 读取输出、发送输入、请求
  manifest，并在界面内切换 channel 过滤。status 区域会显示当前 interactive
  backend 和 FPS。
- `wiremux passthrough --channel N` 会 attach 到一个 mux channel，把按键立即封装为
  `MuxEnvelope(direction=input)` 发送；终端支持时 `Ctrl-]` 退出，通用退出序列是先按 `Esc` 再按 `x`。
- `wiremux tui` 和 `wiremux passthrough` 都支持可选
  `--interactive-backend auto|compat|mio`；默认 `auto` 在 Unix 上优先使用 `mio`，
  不可用时回退到 `compat`，Windows 使用 `compat`。
- `wiremux tui` 支持 `--tui-fps 60|120`；未指定时默认 60 fps，检测到 Ghostty 时
  自动使用 120 fps。
- CLI 显式传入的串口参数只覆盖当前运行，不会自动写回全局配置。

## 输出格式

过滤单个 channel 时，host 不会给 payload 额外添加前缀或换行：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

这类模式适合 console channel，因为 payload 中的 `CRLF`、`CR`、`LF` 会按实际换行显示，
不会被打印成 `\r` 或 `\n` 字符串。

切换 demo console 到 passthrough 后，可以 attach 到 channel 1：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line "mux_console_mode passthrough"
cargo run -- passthrough --port /dev/tty.usbmodem2101 --baud 115200 --channel 1
```

在 passthrough 会话里输入命令并按 Enter，终端支持时 `Ctrl-]` 退出；如果终端把 `Ctrl-]` 当作普通 `]`/`}` 发送，使用 `Esc` 然后 `x` 退出。

不指定 `--channel` 时，host 会保留普通 terminal bytes，并用 `chN> ` 标识 decoded mux
record 的 channel。如果 listen 被动收到设备启动时或命令触发的 manifest，后续输出会用
manifest 里的 channel `name` 显示为 `chN(name)> `；listen 不会主动请求 manifest，
没抓到 manifest 时保持 `chN> `。
ESP32 demo 会在 mux 初始化时和启动后短延迟各输出一次 manifest，方便 macOS USB
serial reset/reconnect 后的被动 listen 捕获 channel name。

```text
ch3(telemetry)> mock stress seq=090 component=wiremux
```

如果一个 channel 的可见行尚未结束就切换到另一个 channel，host 会为可读性插入一个独占
一行的提示：

```text
ch1> booting subsystem
wiremux> continued after partial ch1 line
ch2> sensor ready
```

这行由 host 生成，只表示 display 为避免跨 channel 混行而补了一次展示换行，不代表设备
payload 或协议 decode 错误。

## TUI 模式

TUI 用于交互式调试：

```bash
cd sources/host/wiremux
cargo run -- tui --port /dev/tty.usbmodem2101 --baud 115200
```

快捷键：

- `Ctrl-B` 后按 `0`：无过滤模式，显示普通 terminal bytes 和所有 mux channel。
- `Ctrl-B` 后按 `1..9`：切到对应 channel 的过滤视图。
- `Ctrl-B` 后按 `s`：打开 settings 面板，按 menuconfig 风格编辑物理串口
  profile，并可显式保存为默认配置。
- `Ctrl-B` 后按 `v`：在当前 TUI session 内启用或关闭虚拟串口。
- `Ctrl-B` 后按 `o`：在当前过滤 channel 上切换输入 owner：host 或虚拟串口。
- 鼠标滚轮向上：查看更早的输出，并暂停自动跟随最新日志。
- 鼠标滚轮向下到底部：恢复自动跟随最新日志。
- 拖动输出窗口右侧滚动条：按当前位置查看历史输出或回到底部。
- 输入行为空时连续按两次 `Enter`：恢复自动跟随最新日志。
- `Enter`：发送底部输入行。
- `Esc`：清空底部输入行；passthrough 输入下会在短暂等待后转发为 `Esc`。
- `Ctrl-C`、`Ctrl-]`、或先按 `Esc` 再按 `x`：退出 TUI。

输入路由：

- 无过滤模式下，TUI 为只读视图，不会把输入发送到 channel 1 或任何其他 channel。
- channel 过滤模式下，只有 manifest 声明当前 channel 支持 `DIRECTION_INPUT` 时，
  输入行才会通过当前过滤 channel 发送。
- 底部输入区会显示当前输入状态；只读状态不会累积输入或显示可编辑光标。
- TUI 不会把用户输入作为 raw serial bytes 直接写入串口；host-to-device 输入仍然封装为
  `WMUX` frame + `MuxEnvelope(direction=input)`。
- 如果 manifest 声明当前输入 channel 的 `default_interaction_mode = PASSTHROUGH`，
  TUI 会切换为逐键 passthrough 输入，不等待 `Enter` 聚合成完整命令行。

连接成功后，TUI 会向 system channel 0 发送
`payload_type = "wiremux.v1.DeviceManifestRequest"` 的 manifest 请求。设备返回
`wiremux.v1.DeviceManifest` 后，TUI 会缓存并显示设备、channel 和 max payload 摘要。
输入模式由 manifest 中的 channel interaction mode 决定：未声明或 `LINE` 继续使用
line-mode，`PASSTHROUGH` 使用逐键输入。

## 全局配置与串口 profile

host 工具会读取全局配置文件中的默认物理串口 profile。macOS 默认路径是：

```text
~/Library/Application Support/wiremux/config.toml
```

如果设置了 `XDG_CONFIG_HOME`，则使用：

```text
$XDG_CONFIG_HOME/wiremux/config.toml
```

也可以用 `WIREMUX_CONFIG` 指向一个显式配置文件路径。

配置示例：

```toml
[serial]
port = "/dev/cu.usbserial-0001"
baud = 115200
data_bits = 8
stop_bits = 1
parity = "none"
flow_control = "none"
```

字段含义：

- `port`：真实设备串口路径。
- `baud`：真实设备 transport 的波特率。
- `data_bits`：数据位，支持 `5`、`6`、`7`、`8`。
- `stop_bits`：停止位，支持 `1`、`2`。
- `parity`：校验位，支持 `"none"`、`"odd"`、`"even"`。
- `flow_control`：流控，支持 `"none"`、`"software"`、`"hardware"`。

优先级：

```text
CLI 参数 > 全局配置 > 内置默认
```

因此可以用下面的命令临时覆盖默认配置：

```bash
wiremux tui --port /dev/cu.usbmodem2101 --baud 921600 --data-bits 8 --stop-bits 1 --parity none --flow-control none
```

如果没有传 `--port`，`wiremux tui`、`listen`、`send` 和 `passthrough` 会尝试使用
配置文件里的 `serial.port`。如果 CLI 和配置文件都没有提供 port，命令会报错。

TUI settings 面板只编辑真实物理串口 profile。虚拟串口使用独立的全局配置段，不应
和真实物理串口 profile 混用。generic host 不包含 generic enhanced overlay，因此会
忽略 `[virtual_serial]`，无法启用虚拟串口。省略 `[virtual_serial]` 时，generic
enhanced、vendor enhanced 和 all-feature host 默认启用虚拟串口，并导出 manifest
中的所有 channel；output-only channel 是只读 PTY，input-capable channel 只有在
输入 owner 切换到虚拟串口后才会把写入转发到设备。

```toml
[virtual_serial]
enabled = true
export = "all-manifest-channels"
name_template = "wiremux-{device}-{channel}"
```

Unix/macOS 上真实 PTY 节点仍然由系统分配，但 wiremux 会优先创建稳定 alias：
`/dev/tty.wiremux-<device>-<channel>`。如果 `/dev` 不可写，会 fallback 到用户可写
目录 `/tmp/wiremux/tty`；也可以用 `WIREMUX_VIRTUAL_SERIAL_DIR` 指定 alias 目录。
物理串口断开或 wiremux 正常退出时，alias 会被移除；设备重连并重新收到 manifest
后，同名 alias 会重新创建并指向新的真实 PTY。`kill -9` 不保证清理，下一次启动会
best-effort 替换已经失效的 wiremux symlink。macOS 上关闭虚拟端点时还会
best-effort `revoke(2)` 真实 PTY，让已打开该端点的终端工具更容易感知断开。

settings 面板的视觉和交互约束记录在：

```text
docs/wiremux-tui-menuconfig-style.md
```

## 端口选择

macOS 上 ESP32 USB Serial/JTAG 通常同时出现 `/dev/tty.*` 和 `/dev/cu.*`。如果传入的是 `/dev/tty.usbmodem*`，host 工具会优先尝试配对的 `/dev/cu.usbmodem*`，因为 `cu.*` 更适合应用程序主动连接。

Linux 常见路径是 `/dev/ttyACM0` 或 `/dev/ttyUSB0`。Windows 常见路径是 `COM3`、`COM4` 这类端口名。

不要在代码或脚本里写死 `/dev/tty.usbmodem2101`；它只是本机示例。

## Console line-mode 验收

单终端发送并监听 console channel：

```bash
cd sources/host/wiremux
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

同样方式执行其他命令：

```bash
cd sources/host/wiremux
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_manifest
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_hello
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_utf8
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_stress
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_diag
```

观察其他 channel：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 2 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 3 --line mux_hello
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 4 --line mux_utf8
```

`--channel 2` 应看到 log adapter 输出，`--channel 3` 应看到 telemetry 输出，
`--channel 4` 应看到 UTF-8/emoji demo 输出。
`mux_manifest` 会触发 channel 0 的 protobuf manifest 输出。TUI 也会在连接后主动请求
manifest。
`mux_diag` 会输出 batch/compression 统计，包含 raw bytes、encoded bytes、
ratio、encode_us、decode_ok、fallback_count 和 heap_peak。
`mux_stress` 会向 channel 2 和 channel 3 发送相同的高重复 mock payload，便于在
115200 等实际波特率下比较 heatshrink 与 LZ4。

如果想在一次运行中看到所有 channel，不要传 `--channel`：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --line mux_log
```

## 发布构建

host 工具是单个 Rust 可执行文件：

```bash
cd sources/host/wiremux
cargo build --release
```

构建产物位于：

```text
sources/host/wiremux/target/release/wiremux
```

## 后续计划

- 添加 capture/replay 子命令。
- 增加 TUI 全局配置文件、运行时切换 port/baud，以及可配置快捷键。
- 增加 service/broker 模式，由一个 host 进程独占真实串口并向多个 frontend 分发 channel。
- 在 service/broker 基础上支持 Unix PTY 暴露，让用户用 `screen`、`minicom` 等工具打开单独 channel。
- Windows native virtual COM 支持进入长期 roadmap，短期不作为首期跨平台虚拟设备目标。
