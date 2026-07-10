# CapletRT 开发快速入门

> 本文档帮助你在本地快速搭建 CapletRT 开发环境，并在 QEMU 上运行第一个示例。

## 环境要求

### 必需工具

| 工具 | 版本 | 安装方式 |
|------|------|----------|
| Rust | 1.75+ | [rustup](https://rustup.rs/) |
| QEMU | 8.0+ | `apt install qemu-system-arm` |
| GDB (可选) | arm-none-eabi | `apt install gcc-arm-none-eabi gdb-multiarch` |

### 安装步骤

```bash
# 1. 安装 Rust (如果你还没有)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 添加 ARM Cortex-M 目标
rustup target add thumbv7em-none-eabihf thumbv6m-none-eabi

# 3. 安装 QEMU
# Debian/Ubuntu:
sudo apt install qemu-system-arm

# 4. 安装 ARM 工具链 (可选，用于调试)
sudo apt install gcc-arm-none-eabi gdb-multiarch
```

## 快速开始

### 1. 克隆并构建

```bash
git clone https://github.com/your-repo/CapletRT.git
cd CapletRT

# 构建所有 crates (主机目标)
cargo build

# 构建 hello-qemu-cm4 示例
cargo build -p hello-qemu-cm4 --target thumbv7em-none-eabihf
```

### 2. 在 QEMU 上运行

```bash
# 使用测试脚本 (推荐)
./tests/qemu-cm4/run.sh

# 或手动运行
qemu-system-arm \
    -machine mps2-an500 \
    -nographic \
    -semihosting-config enable=on,target=native \
    -kernel target/thumbv7em-none-eabihf/release/hello-qemu-cm4
```

### 3. 预期输出

```
[INFO] boot: CapletRT v0.1.0 booting on Cortex-M4 @ QEMU (MPS2-AN386)
[INFO] fpu: FPU enabled
[INFO] cap_init: initializing kernel...
[INFO] cap_init: done. Boot task running.
[INFO] demo: hello world
[INFO] sched: idle -> WFI
[INFO] control: tick 1000
[INFO] control: tick 2000
...
```

## 项目结构

```
CapletRT/
├── crates/                    # 核心 crate
│   ├── caplet-kernel/        # 内核核心
│   ├── caplet-cap/           # 能力系统
│   ├── caplet-async/         # async 执行器
│   ├── caplet-hal/           # 硬件抽象层
│   └── ...
├── ports/                    # 架构端口
│   ├── cortex-m/             # ARM Cortex-M
│   ├── cortex-a/             # ARM Cortex-A
│   └── riscv/                # RISC-V (待实现)
├── boards/                   # 板级支持包
│   ├── hello-qemu-cm4/       # QEMU Cortex-M4 示例
│   ├── nucleo-f446ze/        # STM32 Nucleo 板
│   └── ...
├── examples/                  # 示例代码
│   └── caplet-ui-demo/       # UI 演示
└── tests/                    # 集成测试
    └── qemu-cm4/             # QEMU 测试脚本
```

## 开发工作流

### 添加新的 crate

1. 在 `crates/` 目录下创建新 crate
2. 在根目录 `Cargo.toml` 的 `members` 中添加
3. 确保 `#![no_std]` 和 `extern crate alloc`

### 添加新的 Board

1. 在 `boards/` 目录下创建新 board
2. 复制 `boards/hello-qemu-cm4/` 作为模板
3. 修改 `build.rs` 中的链接脚本
4. 更新 `Cargo.toml` 的依赖

### 运行测试

```bash
# 运行 QEMU 集成测试
./tests/qemu-cm4/run.sh

# 运行单元测试
cargo test

# 运行特定 crate 的测试
cargo test -p caplet-kernel
```

### GDB 调试

```bash
# 终端 1: 启动 QEMU 等待 GDB 连接
qemu-system-arm \
    -machine mps2-an500 \
    -nographic \
    -semihosting-config enable=on,target=native \
    -kernel target/thumbv7em-none-eabihf/release/hello-qemu-cm4 \
    -gdb tcp::3333 -S &

# 终端 2: 连接 GDB
arm-none-eabi-gdb \
    target/thumbv7em-none-eabihf/release/hello-qemu-cm4 \
    -ex "target remote :3333"

# GDB 常用命令
(gdb) break rust_main     # 设置断点
(gdb) continue           # 继续执行
(gdb) info registers     # 查看寄存器
(gdb) x/32x 0x20000000   # 查看内存
(gdb) bt                 # 栈回溯
```

## 常见问题

### Q: 编译报错 "AtomicU64 not found"

**A**: 这是因为 Cortex-M 不支持 64 位原子操作。请确保使用 32 位计数器或配置正确的目标架构。

### Q: QEMU 启动后立即 HardFault

**A**: 检查：
1. 链接脚本是否正确配置内存区域
2. 栈大小是否足够
3. 是否有未初始化的指针

### Q: 串口没有输出

**A**: 确保：
1. 使用 `-semihosting-config enable=on,target=native`
2. 检查日志宏是否正确配置

## 下一步

- 阅读 [docv1.md](./docv1.md) 了解 CapletRT 架构
- 查看 `crates/` 中的各 crate 文档
- 参考 `boards/hello-qemu-cm4/` 创建自己的应用

## 参考链接

- [Rust Embedded Book](https://rust-embedded.github.io/book/)
- [cortex-m-rt 文档](https://docs.rs/cortex-m-rt/latest/cortex_m_rt/)
- [QEMU 文档](https://www.qemu.org/docs/master/)
