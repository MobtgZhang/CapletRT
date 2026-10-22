# 仿真与调试矩阵

CapletRT 的板卡定义集中在 `tools/sim/common.sh`。QEMU、Renode、GDB 和
`tests/` 下的脚本共用同一份 `board_spec`，因此不会出现某个后端使用不同
ELF、内存布局或期望输出的情况。

## 板卡矩阵

`all_boards` 当前返回以下 22 个 key：

| key | 包名 | Rust target | QEMU 机型 | Renode 平台 | 功能完成标记 |
|-----|------|-------------|-----------|-------------|--------------|
| `riscv32` | `hello-riscv` | `riscv32imac-unknown-none-elf` | `virt -cpu rv32` | `boards/hello-riscv/renode/rv32.repl` | `CapletRT RISC-V boot OK` |
| `riscv64` | `hello-riscv` | `riscv64gc-unknown-none-elf` | `virt -cpu rv64` | `boards/hello-riscv/renode/rv64.repl` | `CapletRT RISC-V boot OK` |
| `aarch64` | `hello-aarch64-qemu` | `aarch64-unknown-none` | `virt -cpu cortex-a53` | `boards/hello-aarch64-qemu/renode/cortex-a53-gicv3.repl` | `CapletRT AArch64 boot OK` |
| `cortex-a9` | `hello-cortex-a9` | `armv7a-none-eabihf` | `vexpress-a9 -cpu cortex-a9` | `boards/hello-cortex-a9/renode/cortex-a9.repl` | `CapletRT Cortex-A9 boot OK` |
| `a20` | `hello-allwinner-a20-qemu` | `armv7a-none-eabihf` | `cubieboard -cpu cortex-a8` | `boards/hello-allwinner-a20-qemu/renode/allwinner-a20.repl` | `CapletRT Allwinner A20 boot OK` |
| `cm3` | `hello-qemu-cm3` | `thumbv7m-none-eabi` | `mps2-an385 -cpu cortex-m3` | `boards/hello-qemu-cm3/renode/mps2-an385.repl` | `hello world` |
| `cm4` | `hello-qemu-cm4` | `thumbv7em-none-eabihf` | `mps2-an386 -cpu cortex-m4` | `boards/hello-qemu-cm4/renode/mps2-an500.repl` | `hello world` |
| `cm4-fs` | `hello-qemu-cm4-fs` | `thumbv7em-none-eabihf` | `mps2-an386 -cpu cortex-m4` | `boards/hello-qemu-cm4-fs/renode/mps2-an500.repl` | `VFS demo task done` |
| `nucleo-qemu` | `hello-nucleo-f446ze-qemu` | `thumbv7m-none-eabi` | `olimex-stm32-h405` | `boards/hello-nucleo-f446ze-qemu/renode/stm32f446ze.repl` | `Nucleo-F446ZE boot OK` |
| `nucleo` | `nucleo-f446ze` | `thumbv7em-none-eabihf` | `olimex-stm32-h405` | `boards/nucleo-f446ze/renode/nucleo-f446ze.repl` | `Nucleo-F446ZE boot OK` |
| `industrial-demo` | `industrial-demo` | `thumbv7em-none-eabihf` | `mps2-an386` | `boards/industrial-demo/renode/mps2-an500.repl` | `CapletRT M4 industrial scenario` |
| `energy-meter` | `energy-meter` | `thumbv7em-none-eabihf` | `mps2-an386` | `boards/energy-meter/renode/mps2-an500.repl` | `energy-meter: CapletRT M4 industrial scenario` |
| `medical-device` | `medical-device` | `thumbv7em-none-eabihf` | `mps2-an386` | `boards/medical-device/renode/mps2-an500.repl` | `medical-device: CapletRT M4 safety-critical scenario` |
| `automotive` | `automotive` | `thumbv7em-none-eabihf` | `mps2-an386` | `boards/automotive/renode/mps2-an500.repl` | `automotive: CapletRT M4 automotive scenario` |
| `industrial-demo-riscv32` | `industrial-demo` | `riscv32imac-unknown-none-elf` | `virt -cpu rv32` | `boards/industrial-demo/renode/riscv32.repl` | 同上 |
| `industrial-demo-riscv64` | `industrial-demo` | `riscv64gc-unknown-none-elf` | `virt -cpu rv64` | `boards/industrial-demo/renode/riscv64.repl` | 同上 |
| `energy-meter-riscv32` | `energy-meter` | `riscv32imac-unknown-none-elf` | `virt -cpu rv32` | `boards/energy-meter/renode/riscv32.repl` | `energy-meter: ...` |
| `energy-meter-riscv64` | `energy-meter` | `riscv64gc-unknown-none-elf` | `virt -cpu rv64` | `boards/energy-meter/renode/riscv64.repl` | `energy-meter: ...` |
| `medical-device-riscv32` | `medical-device` | `riscv32imac-unknown-none-elf` | `virt -cpu rv32` | `boards/medical-device/renode/riscv32.repl` | `medical-device: ...` |
| `medical-device-riscv64` | `medical-device` | `riscv64gc-unknown-none-elf` | `virt -cpu rv64` | `boards/medical-device/renode/riscv64.repl` | `medical-device: ...` |
| `automotive-riscv32` | `automotive` | `riscv32imac-unknown-none-elf` | `virt -cpu rv32` | `boards/automotive/renode/riscv32.repl` | `automotive: ...` |
| `automotive-riscv64` | `automotive` | `riscv64gc-unknown-none-elf` | `virt -cpu rv64` | `boards/automotive/renode/riscv64.repl` | `automotive: ...` |

## 断言策略

`tools/sim/run_qemu.sh` 和 `tools/sim/run_renode.sh` 都会：

1. 以 release profile 构建对应 target；
2. 在 `SIM_TIMEOUT` 超时保护下启动仿真器；
3. 把完整日志写入 `target/sim/<key>/`；
4. 调用 `assert_board_log()`；
5. 只有全部断言通过才打印 `PASS [<key>] ...`。

每个 key 至少断言一个功能完成标记，并按板卡追加行为断言：

- `cm4` 断言 linkme 任务表 `linked=3 explicit=3 launched=3 skipped=0`；
- `cm4-fs` 断言 LittleFS close/reopen 后内容一致；
- 工业/行业场景断言 MQTT-SN `PUBLISH`/`PUBACK`、`STORAGE persisted`
  （RAM-backed）、看门狗心跳或告警/黑匣子标记；
- Renode 后端还会拒绝 monitor 错误和 unhandled access。

`STORAGE persisted` 不等于 flash 掉电持久化：MPS2 模型没有持久 flash
控制器，行业 demo 的记录保存在 RAM 中，日志会标注 `[ram-backed]`。

超时退出码 `124/137` 只表示仿真器在打印完成标记后继续空闲，不会掩盖断言
失败；缺少预期字符串一定是失败。

## 运行矩阵

```bash
# QEMU：默认 8 s，每个板卡完成后超时终止
SIM_TIMEOUT=15 bash tools/sim/run_matrix.sh

# Renode：启动和断点等待更慢，建议 60 s
SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh

# 只跑指定板卡
SIM_TIMEOUT=15 bash tools/sim/run_matrix.sh cm4 riscv64 industrial-demo-riscv64
SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh aarch64 cortex-a9
```

成功时最后一行分别为：

```text
PASS: QEMU matrix (22 boards)
PASS: Renode matrix (22 boards)
```

## GDB 调试

入口：`tools/sim/gdb_debug.sh <board> [--renode] [gdb args...]`。

脚本不会因为系统里有 `gdb` 就假定它能调试目标：它会执行
`set architecture <target>` 探测，只有真正支持该架构的 GDB 才会被使用。
对 RISC-V / AArch64 / ARM 目标，推荐安装 `gdb-multiarch`。

```bash
# 交互式 QEMU 调试
GDB=gdb-multiarch tools/sim/gdb_debug.sh riscv64

# 交互式 Renode 调试
GDB=gdb-multiarch tools/sim/gdb_debug.sh aarch64 --renode

# 非交互：断点命中后打印寄存器并退出
GDB_BATCH=1 GDB=gdb-multiarch tools/sim/gdb_debug.sh riscv64 \
  -ex 'break rust_main' -ex 'continue' -ex 'info registers pc' -ex 'bt'
```

QEMU 后端通过 `-gdb tcp::<port> -S` 暂停启动；Renode 后端通过
`machine StartGdbServer <port> false` 禁止自动启动，保证客户端可以先安装
早期断点。

已实测的断点目标：

| 架构 | 断点 | 结果 |
|------|------|------|
| RISC-V 64 | `rust_main` | 命中于 `0x800008e0`，回溯到 `riscv_boot` |
| AArch64 | `rust_main` | 命中于 `0x40000398`，回溯到 `aarch64_boot` / `_start` |
| ARMv7-A | `boot_main` | 命中于 `0x60000bec`，回溯到 `_reset` |

## 限制

- Renode 的 Cortex-M4 平台使用 `mps2-an500.repl`，QEMU 使用
  `mps2-an386` 机型；两者都是 Cortex-M4，但外设模型不完全相同。
- STM32F446ZE 的 Renode 平台是兼容模型，不等同于硅片时序。
- `hello-dynamic-loader` 是主机端 ELF/.capp 端到端演示，尚未接入裸机
  boot ROM。
- `caplet-ota` 的 A/B、SHA-256、Ed25519 与回滚有主机端 E2E，但尚未在真实
  flash 上完成掉电注入。
