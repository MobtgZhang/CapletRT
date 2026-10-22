# CapletRT 开发快速入门

本文帮助你在本地搭建 CapletRT 开发环境，并在 QEMU 或 Renode 上运行第一个
示例。完整的 22 板矩阵见 [simulation.md](simulation.md)。

## 环境要求

| 工具 | 用途 | 安装方式 |
|------|------|----------|
| Rust 1.75+ | 构建内核与板卡 | [rustup](https://rustup.rs/) |
| QEMU | ARM/RISC-V 仿真 | `sudo apt install qemu-system-arm qemu-system-misc` |
| Renode 1.17+ | 板级仿真 | 从 [Renode releases](https://github.com/renode/renode/releases) 安装便携版 |
| `gdb-multiarch` | RISC-V/AArch64/ARM 调试 | `sudo apt install gdb-multiarch` |

```bash
rustup target add \
  thumbv7m-none-eabi thumbv7em-none-eabihf \
  armv7a-none-eabihf aarch64-unknown-none \
  riscv32imac-unknown-none-elf riscv64gc-unknown-none-elf
```

## 第一次运行

```bash
git clone https://github.com/your-repo/CapletRT.git
cd CapletRT

# 主机测试与静态检查
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# QEMU Cortex-M4（MPS2-AN386）
tools/sim/run_qemu.sh cm4

# QEMU RISC-V 64（virt）
tools/sim/run_qemu.sh riscv64
```

每个脚本会自动完成 `cargo build --release`、启动仿真器、保存日志并断言输出。
日志位于 `target/sim/<board>/qemu.log` 或 `renode.log`。

Cortex-M4 的期望输出包含：

```text
task_table: linked=3 explicit=3 launched=3 skipped=0
demo: hello world
control: tick 1000
control: tick 2000
```

成功时脚本最后打印：

```text
PASS [cm4] QEMU: hello world
```

## 项目结构

```text
CapletRT/
├── crates/                    # 内核、能力、HAL、FS、网络、MQTT-SN、OTA
├── ports/                     # cortex-m / cortex-a / aarch64 / riscv / allwinner
├── boards/                    # 22 个板卡与行业场景
├── tools/sim/                 # QEMU/Renode/GDB 统一入口
├── tools/kani/                # Kani harness 与 catalogue
├── tests/                     # QEMU/SMP/工业/动态加载 E2E
└── docs/                      # 快速入门、仿真矩阵、安全模型、ADR
```

## 常用命令

### 单板运行

```bash
tools/sim/run_qemu.sh cm3
tools/sim/run_qemu.sh cm4-fs
tools/sim/run_qemu.sh nucleo
tools/sim/run_qemu.sh cortex-a9
tools/sim/run_qemu.sh aarch64
tools/sim/run_qemu.sh riscv32
tools/sim/run_qemu.sh riscv64
tools/sim/run_qemu.sh industrial-demo-riscv64

tools/sim/run_renode.sh cm4
tools/sim/run_renode.sh aarch64
tools/sim/run_renode.sh riscv64
```

### 完整矩阵

```bash
SIM_TIMEOUT=15 bash tools/sim/run_matrix.sh
SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh
```

### 专项测试

```bash
bash tests/qemu-smp-smoke/run.sh          # Cortex-A9 2/4 核 SMP
bash tests/industrial-e2e/run.sh          # MQTT-SN + LittleFS + 看门狗
bash tests/dynamic-loader-e2e/run.sh      # ELF -> .capp -> 执行
bash tools/tickless_verify.sh             # Tickless 计数断言
cargo test -p caplet-ota --test e2e       # OTA A/B 与签名/回滚
```

## 添加新的板卡

1. 在 `boards/` 下创建 crate，提供 `target-none` feature 和裸机入口。
2. 在 `tools/sim/common.sh::board_spec` 中添加 key，声明包名、target、
   QEMU 机型、Renode 平台和完成标记。
3. 在 `all_boards` 中登记 key。
4. 在 `assert_board_log()` 中添加该板卡的功能断言（不能只断言 banner）。
5. 分别运行 `tools/sim/run_qemu.sh <key>` 和 `run_renode.sh <key>`。

## GDB 调试

```bash
# QEMU：启动时暂停，等待 GDB
GDB=gdb-multiarch tools/sim/gdb_debug.sh riscv64

# 常用命令
(gdb) break rust_main
(gdb) continue
(gdb) info registers
(gdb) x/32x 0x80000000
(gdb) bt
```

Renode 后端：

```bash
GDB=gdb-multiarch tools/sim/gdb_debug.sh aarch64 --renode
```

脚本会探测 GDB 是否支持目标架构；若系统 `gdb` 不支持 RISC-V/AArch64/ARM，
会明确失败并提示安装 `gdb-multiarch`，而不会假装连接成功。

## 常见问题

### Q: `cargo test --workspace` 与裸机 target 冲突

**A**：板卡二进制通过 `required-features = ["target-none"]` 隔离，主机测试
不会链接 `cortex-m-rt` 的 `Reset` 符号。运行板卡时使用 `tools/sim/*.sh`，
不要在 workspace 根目录设置全局 `[build] target`。

### Q: Cortex-M3 应该用哪个 target？

**A**：`thumbv7m-none-eabi`。MPS2-AN385 是 ARMv7-M；`caplet-kernel` 依赖
compare-and-swap，`thumbv6m-none-eabi` 不具备所需 atomics。

### Q: 串口没有输出

**A**：优先使用 `tools/sim/run_qemu.sh`，它已经配置
`-semihosting-config enable=on,target=native` 并重定向日志。手动运行时请确认
`-kernel` 指向 workspace 根目录下的 `target/<triple>/release/<package>`。

### Q: Renode 超时但没有 FAIL

**A**：超时只表示固件打印完成标记后继续空闲。脚本会先检查完成标记；
若标记缺失，仍然失败。可提高 `SIM_TIMEOUT`，但不要通过降低断言来“修复”。

## 下一步

- [仿真与调试矩阵](simulation.md)
- [安全模型](security-model.md)
- [Kani 证明](kani.md)
- [工业 demo](industrial-demo.md)
- [移植指南](porting.md)

## 参考链接

- [Rust Embedded Book](https://rust-embedded.github.io/book/)
- [cortex-m-rt 文档](https://docs.rs/cortex-m-rt/latest/cortex_m_rt/)
- [QEMU 文档](https://www.qemu.org/docs/master/)
- [Renode 文档](https://renode.readthedocs.io/)
