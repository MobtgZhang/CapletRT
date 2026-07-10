---
name: CapletRT M4 RISC-V + Complete Ecosystem
overview: "Implement the M4 milestone: RISC-V port + complete ecosystem (crypto/OTA/industry demo). This plan covers four major workstreams: RISC-V architecture port, cryptographic library integration with capability system, OTA secure boot infrastructure, and industry demonstration applications."
todos:
  - id: riscv-1
    content: 创建 ports/riscv crate 框架（Cargo.toml、lib.rs、目录结构）
    status: completed
  - id: riscv-2
    content: 实现 Arch trait for RISC-V（arch.rs）
    status: completed
  - id: riscv-3
    content: 实现 TaskContext + context switch 内联汇编（context.rs）
    status: completed
  - id: riscv-4
    content: 实现 boot + BSS 清零 + PMP 配置（boot.rs, mmu.rs）
    status: completed
  - id: riscv-5
    content: 实现 CLINT + PLIC 中断控制器（clint.rs, plic.rs）
    status: completed
  - id: riscv-6
    content: 实现 UART 驱动 + tick + panic（uart.rs, tick.rs, panic.rs）
    status: completed
  - id: riscv-7
    content: 配置 .cargo/config.toml RISC-V target + QEMU runner
    status: completed
  - id: riscv-8
    content: 创建 boards/hello-riscv 最小 hello-world
    status: completed
  - id: crypto-1
    content: 实现 Crypto Syscall Handler（syscall_crypto.rs）
    status: completed
  - id: crypto-2
    content: 实现 Flash Syscall Handler（syscall_flash.rs）
    status: completed
  - id: crypto-3
    content: 集成 crypto_verify() 使用 ed25519-dalek
    status: completed
  - id: crypto-4
    content: RISC-V crypto extension 支持 + Crypto HAL trait
    status: completed
  - id: crypto-5
    content: 集成 CryptoOp/FlashOp 到 syscall_dispatch
    status: completed
  - id: ota-1
    content: 定义 Flash 分区布局（FlashLayout struct）
    status: completed
  - id: ota-2
    content: 创建 crates/caplet-ota crate（BootManager、A/B 切换）
    status: completed
  - id: ota-3
    content: 实现安全启动 bootloader（签名验证 + hash 校验）
    status: completed
  - id: ota-4
    content: 完善 Loader 签名验证（feature flag crypto-hardware）
    status: completed
  - id: ota-5
    content: 实现 OTA 完整性校验（SHA-256 + Ed25519）
    status: completed
  - id: industry-1
    content: 扩展 boards/industrial-demo 为完整 MQTT-SN 应用
    status: completed
  - id: industry-2
    content: 创建 boards/energy-meter（电力监测场景）
    status: completed
  - id: industry-3
    content: 创建 boards/medical-device（医疗设备场景）
    status: completed
  - id: industry-4
    content: 创建 boards/automotive（汽车辅助场景）
    status: completed
  - id: industry-5
    content: 所有 industry boards 添加 RISC-V target 支持
    status: completed
isProject: false
---

## M4：RISC-V + 完整生态（2027 Q3+）

M4 有四个子目标，以下按依赖顺序规划。

---

## TODO 1：RISC-V Port（最优先）

### 1.1 创建 `ports/riscv/` crate

创建目录结构和基础文件：

```
ports/riscv/
├── Cargo.toml
├── src/
│   ├── lib.rs          # 模块根，re-export
│   ├── arch.rs         # 实现 Arch trait for RiscV
│   ├── context.rs      # TaskContext + 内联汇编
│   ├── boot.rs         # riscv_boot() + BSS 清零
│   ├── mmu.rs          # PMP（无 MMU）或 Sv39（带 MMU）
│   ├── clint.rs        # Core-Local Interruptor (mtimecmp)
│   ├── plic.rs         # Platform-Level Interrupt Controller
│   ├── uart.rs         # NS16550a 或 LiteX UART
│   ├── tick.rs         # now_ticks() + tickless idle
│   ├── panic.rs        # panic_halt
│   └── kernel_bridge.rs # rust_main → caplet_kernel::cap_init()
├── link.x              # 链接脚本（RISC-V 版）
└── memory.x            # QEMU virt 内存布局
```

### 1.2 实现 `Arch` trait

在 [`ports/riscv/src/arch.rs`](ports/riscv/src/arch.rs)（新建）中实现 `Arch` trait，参照 [`crates/caplet-kernel/src/arch.rs`](crates/caplet-kernel/src/arch.rs:67) 的 trait 定义：

| 方法 | RISC-V 实现 |
|------|------------|
| `name()` | `"riscv32"` 或 `"riscv64"` |
| `max_cpus()` | 1（QEMU virt 单核），支持 SMP 可扩展 |
| `early_init()` | 关 mie，写 mtvec，配置 PMP |
| `enable_mmu(cfg)` | Sv39 页表（`ports/riscv/src/mmu.rs`），无 MMU 时 no-op |
| `context_switch(prev, next)` | 保存 s0-s11（callee-saved），切换 sp |
| `start_first_task(next) -> !` | mret 跳入第一个任务 |
| `ipi_send(target, reason, arg)` | CLINT msip 寄存器写 |
| `fence()` | `fence i,r` 或 `sfence.vma` |
| `now_ticks()` | 读 mtime（64-bit）|
| `request_reschedule()` | 软件中断（CLINT）|
| `wait_for_interrupt()` | `wfi` |

### 1.3 Context Switch 内联汇编

参照 [`ports/cortex-m/src/context.rs`](ports/cortex-m/src/context.rs) 的设计，RISC-V 版本：

- **TaskContext 结构**：保存 s0-s11（12 个 callee-saved 寄存器）+ sepc + sstatus
- **__caplet_switch**：保存当前 s0-s11 到旧栈 → 更新 sp → 从新栈恢复 s0-s11 → `sret`
- **build_initial_stack**：手动构建完整栈帧，sepc = 入口，sstatus.mie = 0
- **栈增长方向**：向低地址增长

```rust
// ports/riscv/src/context.rs 核心设计
#[repr(C)]
struct TaskContext {
    s0: u32, s1: u32, s2: u32, s3: u32,
    s4: u32, s5: u32, s6: u32, s7: u32,
    s8: u32, s9: u32, s10: u32, s11: u32,
    sepc: u32,  // 保存的 pc
    sstatus: u32, // 保存的 mstatus（mie 位等）
}
```

### 1.4 中断控制器

- **PLIC**（[`ports/riscv/src/plic.rs`](ports/riscv/src/plic.rs)）：Platform-Level Interrupt Controller，处理外部中断（UART、网络等）。参照 ARM 的 GIC 设计（[`ports/cortex-a/src/gic.rs`](ports/cortex-a/src/gic.rs)）
- **CLINT**（[`ports/riscv/src/clint.rs`](ports/riscv/src/clint.rs)）：Core-Local Interruptor，处理软件中断、定时器中断（M/S 模式）

### 1.5 MMU 支持

- **无 MMU 模式**（QEMU virt 默认）：使用 PMP（Physical Memory Protection）寄存器做权限隔离
- **有 MMU 模式**（远期）：实现 Sv39 页表，参照 [`ports/cortex-a/src/mmu.rs`](ports/cortex-a/src/mmu.rs) 的设计模式

### 1.6 QEMU runner 配置

在 `.cargo/config.toml` 新增 RISC-V target：

```toml
[target.riscv32imac-unknown-none-elf]
runner = "qemu-system-riscv32 -machine virt -cpu rv32 -nographic -bios none -kernel"

[target.riscv64gc-unknown-none-elf]
runner = "qemu-system-riscv64 -machine virt -cpu rv64gc -nographic -bios none -kernel"
```

### 1.7 最小 hello-world board

创建 `boards/hello-riscv/`：

```
boards/hello-riscv/
├── Cargo.toml
├── src/main.rs      # cortex_m_rt::entry，调用 kernel_init()
├── build.rs
├── .cargo/config.toml
└── memory.x
```

目标：QEMU virt 上 `cargo run` 输出 "hello world"。

---

## TODO 2：加密库集成（Crypto 能力协作）

### 2.1 实现 Crypto Syscall Handler

当前 [`crates/caplet-security/src/crypto.rs`](crates/caplet-security/src/crypto.rs) 只有类型定义，需要实现真实操作：

新增 [`crates/caplet-kernel/src/syscall_crypto.rs`](crates/caplet-kernel/src/syscall_crypto.rs)（新建）：

- 新增 syscall 编号 `Syscall::CryptoOp`
- 处理 AES-128 ECB/CBC 加解密、SHA-256、TRNG
- 调用平台 crypto driver（通过 trait）

```rust
// syscall_crypto.rs
pub fn handle_crypto_syscall(args: SyscallArgs) -> SyscallResult {
    let op: CryptoOp = unsafe { core::mem::transmute(args.arg0) };
    match op {
        CryptoOp::Aes128EcbEncrypt { dst, src, key } => { /* ... */ }
        CryptoOp::Sha256 { dst, src } => { /* ... */ }
        CryptoOp::Trng128 { dst } => { /* ... */ }
    }
}
```

### 2.2 集成到 syscall dispatch

在 [`crates/caplet-kernel/src/syscall_dispatch.rs`](crates/caplet-kernel/src/syscall_dispatch.rs:16) 中新增分支：

```rust
Syscall::CryptoOp => handle_crypto_syscall(args),
Syscall::FlashRead | Syscall::FlashWrite | Syscall::FlashErase => handle_flash_syscall(args),
```

### 2.3 实现 Crypto Token 验证

当前 [`crates/caplet-loader/src/verifier.rs`](crates/caplet-loader/src/verifier.rs) 中 `crypto_verify()` 返回 `Unavailable`。需要：

- 集成 `ed25519-dalek` 或 `ring` crate
- 实现 `verify_signature(public_key, message, signature) -> bool`
- 在 `caplet-loader` 中为 `.capp` 格式启用签名验证

### 2.4 RISC-V Crypto Extension 支持

- 检测 RISC-V 是否有 `k` 扩展（crypto）
- 如果有，使用硬件 crypto 指令
- 如果没有，使用纯软件实现（`aes` / `sha256` crate）

### 2.5 Crypto HAL trait

在 [`crates/caplet-hal/src/lib.rs`](crates/caplet-hal/src/lib.rs) 新增 crypto trait：

```rust
pub trait Crypto {
    fn aes128_ecb_encrypt(&mut self, key: &[u8; 16], data: &mut [u8]);
    fn sha256_update(&mut self, data: &[u8]) -> [u8; 32];
    fn trng_read(&mut self) -> [u8; 16];
}
```

---

## TODO 3：OTA / 安全启动

### 3.1 Flash 分区布局

定义标准 flash 分区（所有平台通用）：

| 区域 | 大小 | 用途 |
|------|------|------|
| Bootloader | 32~64 KB | 安全启动 + OTA 决策 |
| Primary | 动态 | 运行固件 A |
| Secondary | 动态 | 固件 B（OTA 升级目标） |
| Config | 8~16 KB | 持久化配置 |
| Audit Log | 8~16 KB | 安全审计日志 |

在 [`crates/caplet-security/src/flash.rs`](crates/caplet-security/src/flash.rs) 新增分区定义：

```rust
pub struct FlashLayout {
    pub bootloader: FlashRegion,
    pub primary: FlashRegion,
    pub secondary: FlashRegion,
    pub config: FlashRegion,
    pub audit: FlashRegion,
}
```

### 3.2 实现 Flash Syscall Handler

新建 [`crates/caplet-kernel/src/syscall_flash.rs`](crates/caplet-kernel/src/syscall_flash.rs)（新建）：

- `sys_flash_read(offset, buf)` — 从 flash 读取
- `sys_flash_write(offset, buf)` — 写入（需要 FlashToken）
- `sys_flash_erase(sector)` — 擦除扇区
- 权限检查：只能写 secondary 区域或 config 区域

集成到 syscall dispatch（TODO 2.2 中已覆盖）。

### 3.3 OTA Manager

创建 `crates/caplet-ota/` crate（新建）：

```
crates/caplet-ota/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── boot_manager.rs  # A/B 切换逻辑
│   ├── downloader.rs    # HTTP/CoAP 下载固件
│   ├── verifier.rs     # 签名验证 + 完整性校验（SHA-256）
│   └── rollback.rs     # 回滚机制
└── src/bootloader.rs  # 安全启动固件（独立编译）
```

**boot_manager.rs 核心逻辑**：

```rust
pub enum BootSlot {
    Primary,
    Secondary,
}

pub struct BootManager;
impl BootManager {
    pub fn select_slot() -> BootSlot {
        // 读取 boot counter + magic
        // 如果 primary 有效且版本 >= secondary → Primary
        // 否则 → Secondary
    }
    
    pub fn mark_booted(slot: BootSlot) {
        // 写 magic，标记已启动
    }
    
    pub fn mark_update_success(slot: BootSlot) {
        // 更新成功，擦除 magic
    }
    
    pub fn mark_update_failed(slot: BootSlot) {
        // 触发回滚
    }
}
```

### 3.4 安全启动流程

在 `crates/caplet-ota/src/bootloader.rs` 中实现：

1. 上电 → bootloader 入口
2. 读取 flash header（magic + version + hash）
3. 验证 primary 固件签名（Ed25519）
4. 计算固件 SHA-256，与 header 中存储的 hash 比对
5. 验证通过 → 跳转到 primary
6. 验证失败 → 跳转到 secondary（或 halt）

### 3.5 完善 Loader 签名验证

当前 [`crates/caplet-loader/src/sandbox.rs`](crates/caplet-loader/src/sandbox.rs) 的 `load_module()` 中：

- 当 `crypto_verify()` 返回 `Unavailable` 时跳过验证
- 改为：提供 feature flag `crypto-hardware`，启用时调用真实 crypto
- 为 RISC-V 平台启用此 feature flag

### 3.6 OTA 完整性校验

使用 SHA-256 校验下载固件完整性：

```rust
// crates/caplet-ota/src/verifier.rs
pub fn verify_firmware(firmware: &[u8], signature: &[u8; 64], public_key: &[u8; 32]) -> bool {
    let hash = sha256(firmware);
    ed25519_verify(signature, &hash, public_key)
}
```

---

## TODO 4：行业内示范应用

### 4.1 扩展工业 Demo 为完整 MQTT-SN 应用

当前 [`boards/industrial-demo/src/main.rs`](boards/industrial-demo/src/main.rs) 是简化版，需要扩展为真实场景：

```rust
// boards/industrial-demo/src/main.rs 扩展
async fn main(spawner: Spawner, caps: Caplets) {
    let uart = caps.service("uart:0").unwrap();
    let flash = caps.service("flash:0").unwrap();
    let socket = caps.service("udp:1883").unwrap();
    
    // 传感器采集任务（优先级 3）
    spawner.spawn(sensor_task(uart));
    
    // 存储任务（优先级 2）— 数据本地持久化
    spawner.spawn(storage_task(flash));
    
    // 上报任务（优先级 1）— MQTT-SN Publish
    spawner.spawn(mqtt_sn_publish(socket));
    
    // 看门狗任务（最高优先级）— 系统健康监控
    spawner.spawn(watchdog_task());
}
```

### 4.2 创建行业场景 Board

在 `boards/` 下新增场景化 board：

```
boards/
├── industrial-demo/      # 现有：传感器 + 存储 + MQTT-SN
├── energy-meter/         # 新增：电力监测（功率采样、谐波分析）
├── medical-device/       # 新增：医疗设备（安全关键数据采集）
└── automotive/           # 新增：汽车辅助（CAN 总线、轻量时间同步）
```

### 4.3 能源监测应用（`boards/energy-meter/`）

- 模拟电压/电流采样（ADC）
- 计算有功/无功功率、功率因数
- 数据本地存储（LittleFS）
- 定时上报（MQTT-SN）
- 安全：flash 写入能力仅授予 storage task

### 4.4 医疗设备应用（`boards/medical-device/`）

- 关键生理参数采集（模拟心率、血氧）
- 异常阈值告警
- 安全：双备份存储 + 看门狗
- 能力模型演示：心率传感器能力 → 派生 → 限速只读能力 → 医疗监控服务

### 4.5 汽车辅助应用（`boards/automotive/`）

- CAN 总线消息收发（模拟）
- 轻量时间同步（IEEE 802.1AS 简化版）
- 故障记录（黑匣子，追加写入 flash）
- 能力模型：CAN 外设能力仅授予动力系统监控任务

### 4.6 RISC-V 兼容版本

确保所有 industry boards 支持 RISC-V target：

```toml
# boards/industrial-demo/Cargo.toml
[target.riscv32imac-unknown-none-elf]
rustflags = ["-C", "link-arg=-Tlink.x"]
```

---

## 实施顺序

```
阶段 1（RISC-V 基础）
  ├─ TODO 1.1 创建 ports/riscv 框架
  ├─ TODO 1.2 Arch trait 实现
  ├─ TODO 1.3 Context switch + boot
  ├─ TODO 1.6 QEMU runner
  └─ TODO 1.7 boards/hello-riscv

阶段 2（Crypto + Flash 能力）
  ├─ TODO 2.1 Crypto syscall handler
  ├─ TODO 2.2 Flash syscall handler
  ├─ TODO 2.3 实现 crypto_verify()
  ├─ TODO 2.4 RISC-V crypto extension
  └─ TODO 2.5 Crypto HAL trait

阶段 3（OTA 安全启动）
  ├─ TODO 3.1 Flash 分区布局
  ├─ TODO 3.3 OTA Manager
  ├─ TODO 3.4 安全启动 bootloader
  ├─ TODO 3.5 Loader 签名验证
  └─ TODO 3.6 OTA 完整性校验

阶段 4（行业示范应用）
  ├─ TODO 4.1 扩展 industrial-demo
  ├─ TODO 4.3 boards/energy-meter
  ├─ TODO 4.4 boards/medical-device
  ├─ TODO 4.5 boards/automotive
  └─ TODO 4.6 RISC-V 兼容版本
```

---

## 关键技术决策

**RISC-V ISA 选择**：初期使用 RV32IMAC（整数 + 乘除 + atomics）。QEMU virt 支持此配置。

**OTA 双区策略**：使用 dual-bank flash，支持 A/B 无缝切换，不支持 in-place update（掉电不安全）。

**Crypto 库选择**：
- 软件实现：`aes` + `sha2` + `ed25519-dalek` crates（`no_std` 兼容）
- 硬件加速：通过 `Crypto` HAL trait 在 RISC-V crypto extension 或外设上实现

**行业应用优先级**：工业 demo（已有）→ 能源监测（新增）→ 医疗设备（新增）→ 汽车辅助（新增）