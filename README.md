# CapletRT

面向嵌入式实时场景、以“能力（Capability）”为核心安全原语、用 Rust 写成的工业级 RTOS 内核。

## 项目愿景

- **能力模型**：借鉴 Symbian / seL4 的 Capability 安全原语。
- **目标硬件**：ARM Cortex-M、ARM Cortex-A（ARM32/AArch64）、RISC-V 32/64。
- **仿真与调试**：同一套板级配置同时支持 QEMU 与 Renode，并可通过 GDB 远程调试。
- **设计哲学**：外层 API 轻、内核重。

## 支持平台

当前 `tools/sim/common.sh` 定义了 22 个可执行板卡 key：

| 架构 | 板卡 key | 说明 |
|------|----------|------|
| RISC-V 32 | `riscv32`、`industrial-demo-riscv32`、`energy-meter-riscv32`、`medical-device-riscv32`、`automotive-riscv32` | QEMU `virt` / Renode RISC-V virt |
| RISC-V 64 | `riscv64`、`industrial-demo-riscv64`、`energy-meter-riscv64`、`medical-device-riscv64`、`automotive-riscv64` | QEMU `virt` / Renode RISC-V virt |
| AArch64 | `aarch64` | QEMU `virt` / Renode Cortex-A53 + GICv3 |
| ARMv7-A | `cortex-a9`、`a20` | QEMU `vexpress-a9` / `cubieboard`，Renode Cortex-A9 / A20 |
| ARMv7-M | `cm3`、`nucleo-qemu` | MPS2-AN385、STM32F4 兼容 QEMU 机型 |
| ARMv7E-M | `cm4`、`cm4-fs`、`nucleo`、`industrial-demo`、`energy-meter`、`medical-device`、`automotive` | MPS2 / STM32F446ZE / 行业场景 |

完整的构建目标、QEMU 机型、Renode 平台和期望输出见 [docs/simulation.md](docs/simulation.md)。

## 快速开始

### 前置要求

```bash
# Rust 与裸机 target
rustup target add \
  thumbv7m-none-eabi thumbv7em-none-eabihf \
  armv7a-none-eabihf aarch64-unknown-none \
  riscv32imac-unknown-none-elf riscv64gc-unknown-none-elf

# Debian/Ubuntu：QEMU（ARM 与 RISC-V 机型）
sudo apt-get install qemu-system-arm qemu-system-misc

# 可选：多架构 GDB 调试
sudo apt-get install gdb-multiarch

# 可选：Renode（从官方 release 安装便携版）
# https://github.com/renode/renode/releases
```

### 运行单个板卡

```bash
# QEMU：Cortex-M4
tools/sim/run_qemu.sh cm4

# QEMU：RISC-V 64
tools/sim/run_qemu.sh riscv64

# Renode：AArch64
tools/sim/run_renode.sh aarch64
```

脚本会构建 release ELF、在超时保护下启动仿真器、保存完整日志到
`target/sim/<board>/`，并对板卡功能完成标记做硬断言；只有断言通过才会打印
`PASS [<board>] ...`。

### 运行完整矩阵

```bash
# 22 个板卡的 QEMU 矩阵
SIM_TIMEOUT=15 bash tools/sim/run_matrix.sh

# 22 个板卡的 Renode 矩阵
SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh
```

### 主机测试与静态检查

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo bench --no-run --workspace
```

## GDB 调试

`tools/sim/gdb_debug.sh` 会检查所选 GDB 是否真的理解目标架构，而不是只检查
二进制是否存在。使用支持多架构的 GDB：

```bash
GDB=$(command -v gdb-multiarch) tools/sim/gdb_debug.sh riscv64
GDB=$(command -v gdb-multiarch) tools/sim/gdb_debug.sh aarch64
GDB=$(command -v gdb-multiarch) tools/sim/gdb_debug.sh cortex-a9
```

批量验证断点命中：

```bash
GDB_BATCH=1 GDB=$(command -v gdb-multiarch) \
  tools/sim/gdb_debug.sh riscv64 \
  -ex 'break rust_main' -ex 'continue' -ex 'info registers pc'
```

Renode 后端使用相同入口，追加 `--renode` 即可。

## 项目结构

```
CapletRT/
├── crates/                    # 内核、能力、异步、HAL、文件系统、网络、MQTT-SN、OTA 等
├── ports/                     # 架构端口
│   ├── cortex-m/              # ARMv7-M / ARMv7E-M
│   ├── cortex-a/              # ARMv7-A、MMU、SMP
│   ├── aarch64/               # AArch64、GICv3、异常级别
│   ├── riscv/                 # RISC-V 32/64、CLINT/PLIC、PMP
│   └── allwinner/             # Allwinner A20 板级辅助
├── boards/                    # 22 个可仿真板卡（RISC-V/Cortex-A/Cortex-M/行业场景）
├── tools/sim/                 # QEMU/Renode/GDB 统一矩阵与断言
├── tools/kani/                # 39 条 Kani 证明
├── tests/                     # QEMU、SMP、工业、动态加载等端到端测试
├── docs/                      # 快速入门、仿真矩阵、安全模型、Kani、ADR
└── .github/workflows/         # CI：host、Kani、QEMU、Renode、GDB
```

## 核心概念

### 能力模型（Capability）

```rust
let uart = caplet::service("uart:1")?;
uart.write_all(b"hello\n").await?;
```

### 任务模型

```rust
#[caplet::main]
async fn main(spawner: Spawner, caps: Caplets) {
    spawner.spawn(my_task());
}

#[caplet::task(priority = 3, stack = 4096)]
async fn my_task() {
    // Task implementation
}
```

### 调度器

- 静态优先级 0–31（0 最高），FIFO / Round-Robin 与时间片预算。
- Tickless idle、timer wheel、真实 WFI。
- Cortex-A 支持 PSCI secondary 启动、共享 L1 页表与按亲和性迁移的 SMP 调度。

## 里程碑状态

- [x] **M1**：能力模型、CSpace、真实 waker、调度器、syscall、CoE 启动路径。
- [x] **M2**：HAL、STM32F4/nRF52、Nucleo-F446ZE、配额、Tickless、VFS/LittleFS/FAT32、网络栈、MQTT-SN、工业 demo、Kani 目录。
- [x] **M3**：Cortex-A9 MMU/进程模型、SMP、命名空间 IPC、ELF/.capp 动态加载、签名校验。
- [x] **M4**：RISC-V 32/64、AArch64、Crypto/Flash syscall、OTA A/B 与安全启动、四个行业场景的 RISC-V 变体。

验证边界：QEMU/Renode 覆盖裸机启动与板级功能；OTA 与动态加载已有主机端到端
验证，但尚未接入真实板卡的 boot ROM；性能门目前只提供主机基准，未在
`mps2-an500` 上完成计时。

## 许可证

MIT License - 详见 [LICENSE](LICENSE)。
