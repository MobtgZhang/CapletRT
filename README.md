# CapletRT

面向嵌入式实时场景、以"能力（Capability）"为核心安全原语、用 Rust 写成的工业级 RTOS 内核。

## 项目愿景

CapletRT 是一个面向嵌入式实时场景、以"能力（Capability）"为核心安全原语、用 Rust 写成的工业级 RTOS 内核。

- **能力模型**：借鉴 Symbian / seL4 的 Capability 安全原语
- **目标硬件**：Cortex-M（Cortex-A、RISC-V 远期）
- **设计哲学**：外层 API 轻、内核重

## 快速开始

### 前置要求

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装 ARM target
rustup target add thumbv6m-none-eabi thumbv7em-none-eabihf

# 安装 QEMU
sudo apt-get install qemu-system-arm
```

### 编译运行

```bash
# 克隆项目
git clone https://github.com/your-username/CapletRT.git
cd CapletRT

# 构建 Cortex-M3 版本
cd boards/hello-qemu-cm3
cargo build --release

# 构建 Cortex-M4 版本
cd ../hello-qemu-cm4
cargo build --release
```

### 在 QEMU 中运行

```bash
# 运行 CM3 版本
qemu-system-arm -machine mps2-an385 -cpu cortex-m3 -nographic \
    -semihosting-config enable=on,target=native \
    -kernel target/thumbv6m-none-eabi/release/hello-qemu-cm3.elf

# 运行 CM4 版本
qemu-system-arm -machine mps2-an500 -cpu cortex-m4 -nographic \
    -semihosting-config enable=on,target=native \
    -kernel target/thumbv7em-none-eabihf/release/hello-qemu-cm4.elf
```

## 项目结构

```
CapletRT/
├── crates/              # 核心 crate
│   ├── caplet-macros/  # 过程宏
│   ├── caplet-cap/     # 能力原语
│   ├── caplet-task/    # 任务抽象
│   ├── caplet-async/   # Stackless async 执行器
│   ├── caplet-sync/     # 同步原语
│   ├── caplet-kernel/  # 内核核心
│   ├── caplet-hal/     # HAL trait 定义
│   └── caplet-log/     # 日志框架
├── ports/              # 架构端口
│   └── cortex-m/       # Cortex-M 端口
├── boards/             # 板级支持
│   ├── hello-qemu-cm3/ # QEMU CM3 demo
│   └── hello-qemu-cm4/ # QEMU CM4 demo
├── tests/              # 集成测试
├── docs/               # 文档
└── .github/workflows/  # CI 配置
```

## 核心概念

### 能力模型 (Capability)

CapletRT 使用能力作为资源访问的核心机制：

```rust
// 获取服务能力
let uart = caplet::service("uart:1")?;
uart.write_all(b"hello\n").await?;
```

### 任务模型

支持 Stackless Async 作为默认并发模型：

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

静态优先级调度器，O(1) 时间复杂度：

- 优先级：0-31（0 最高）
- 支持 FIFO 和 Round-Robin
- Tickless idle 支持

## 里程碑

- [x] **M1** (2026 Q4): 模型完整 + 可跑
  - 核心类型定义
  - CSpace 实现
  - Stackless async 执行器
  - 静态优先级调度器
  - QEMU CM3/CM4 端口

- [ ] **M2** (2027 Q1): 可形式化 + 工业 demo
  - Kani 覆盖
  - HAL trait 实现
  - LittleFS / FAT32
  - TCP/IP 协议栈

- [ ] **M3** (2027 Q2): 动静态双加载 + 多核
  - [x] Step 1: Arch trait + per-CPU Scheduler 抽象
  - [x] Step 2: ports/cortex-a 最小骨架
  - [x] Step 3: Cortex-A9 MMU L1 section 表
  - [x] Step 4: 真进程模型 Process
  - [x] Step 5: Cap<MemoryRegion> ↔ MMU 同步 syscall
  - [x] Step 6: caplet-sync TicketLock 升级
  - [x] Step 7: SMP 启动（PSCI + secondary_main + GIC SGI）
  - [x] Step 8: SmpScheduler 完整化
  - [x] Step 9: IpcNamespace + 5 个 syscall
  - [x] Step 10: caplet-loader crate 骨架
  - [x] Step 11: ELF 子集解析器
  - [x] Step 12: .capp 容器格式
  - [x] Step 13: ed25519 签名校验
  - [x] Step 14: load_module 完整流程
  - [x] Step 15: hello-dynamic-loader demo (cargo run 跑通)
  - [x] Step 16: Loader metrics + audit ring (`caplet_loader::metrics`)
  - [x] Step 17: 单元测试扩展（loader 17 + cortex-a 5 + smp 3 + ipc 8）
  - [x] Step 18: Kani harness 扩展（loader_manifest.rs 加入）
  - [x] Step 19: CI 增加 cortex-a9 / loader / hello-dynamic-loader jobs
  - [x] Step 20: bench-harness 扩展（loader_throughput.rs, 378 ns/op）
  - [x] Step 21: ADR 0010–0013 + dev-quickstart 更新
  - [x] Step 22: 最终验收（workspace 测试 0 失败 + 验收清单逐项勾选）

- [ ] **M4** (2027 Q3+): RISC-V + 完整生态

## 许可证

MIT License - 详见 [LICENSE](LICENSE)
