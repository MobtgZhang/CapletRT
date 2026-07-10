---
name: M3 动静态双加载 + 多核 + 真进程（顺序执行版）
overview: 按线性步骤实现 docv1.md §12 M3 阶段：双加载（静态 + ELF 子集 + .capp 容器）、Cortex-A9 真 MMU 进程隔离、SMP 多核调度、跨 CSpace 命名空间 IPC。在 QEMU vexpress-a9 上跑通端到端 demo，CI 扩展到多板矩阵。
todos:
  - id: step1-arch-abstraction
    content: "Step 1: 抽取 Arch trait + MAX_CPUS/CpuId + per-CPU Scheduler 抽象"
    status: completed
  - id: step2-cortex-a9-hello
    content: "Step 2: ports/cortex-a 最小骨架（boot/panic/PL011/SVC 切换到 EL1），QEMU vexpress-a9 跑通 hello world"
    status: completed
  - id: step3-mmu-section
    content: "Step 3: 落地 Cortex-A9 MMU（L1 section 表 + map/unmap/tlbi）"
    status: completed
  - id: step4-process-model
    content: "Step 4: 真进程模型（Process、Kernel.processes、删除 SHARED_CSPACE_PTR）"
    status: completed
  - id: step5-cap-mmu-sync
    content: "Step 5: Cap<MemoryRegion> ↔ MMU 同步 + MapMemory/UnmapMemory/RegionGrant syscall + 安全位"
    status: completed
  - id: step6-ticket-lock
    content: "Step 6: caplet-sync 改 TicketLock，单核零开销回退"
    status: completed
  - id: step7-smp-boot
    content: "Step 7: SMP 启动（PSCI cpu_on + secondary_main + GIC SGI）"
    status: completed
  - id: step8-smp-scheduler
    content: "Step 8: SmpScheduler 完整化（per-CPU run queue + 负载均衡 + IPI reschedule）"
    status: completed
  - id: step9-ipc-namespace
    content: "Step 9: IpcNamespace + NsCreate/NsJoin/NsPublish/NsSubscribe/NsEndpoint syscall + Endpoint 派生"
    status: completed
  - id: step10-loader-skeleton
    content: "Step 10: 新建 caplet-loader crate（ModuleLoader trait + Allocator + Sandbox 框架）"
    status: completed
  - id: step11-elf-subset
    content: "Step 11: ELF 子集解析器（白名单 R_ARM_RELATIVE + 拒绝 PT_DYNAMIC）"
    status: completed
  - id: step12-capp-format
    content: "Step 12: 自定义 .capp 容器格式（CAPP magic + 紧凑布局 + cap manifest）"
    status: completed
  - id: step13-loader-verifier
    content: "Step 13: ed25519-dalek 签名校验 + key id allowlist + granted/required 一致性"
    status: completed
  - id: step14-loader-flow
    content: "Step 14: load_module 完整流程（alloc→copy→reloc→cspace→process→cap 返回）"
    status: completed
  - id: step15-dynamic-demo
    content: "Step 15: boards/hello-dynamic-loader（静态嵌入 .capp + ELF 双模式并存）"
    status: completed
  - id: step16-loader-observability
    content: "Step 16: Loader metrics + audit log ring buffer"
    status: completed
  - id: step17-tests
    content: "Step 17: 单元测试（loader_smoke、smp_smoke host、kernel syscall 扩展）"
    status: completed
  - id: step18-kani
    content: "Step 18: Kani harness 扩展（loader_manifest、mmu_permissions、smp_ipc）"
    status: completed
  - id: step19-ci
    content: "Step 19: .github/workflows/ci.yml 增加 cortex-a9 / loader / hello-dynamic-loader jobs"
    status: completed
  - id: step20-bench
    content: "Step 20: bench-harness 增加 loader_throughput / smp_ipc_latency / mmu_map_unmap"
    status: completed
  - id: step21-adrs
    content: "Step 21: 写 ADR 0010–0013 + 更新 README/dev-quickstart/security-model"
    status: completed
  - id: step22-final-gate
    content: "Step 22: 最终验收（cargo test/clippy 全绿 + 验收清单逐项勾选 + README 状态表更新）"
    status: completed
isProject: false
---

# M3: 动静态双加载 + 多核 + 真进程（2027 Q2，顺序执行版）

> 本计划是 `docv1.md §12 M3` 的可执行版。原 Phase 1–9 被压平成 **Step 1 → Step 22 的线性流水线**：每一步只依赖之前步骤的产物，每一步完成后必须满足 §“每步门禁”。

## 执行路线图（顺序流水线）

按顺序执行，下面任何一步开始前必须先完成上一步：

1. **Step 1  架构抽象**：抽 `Arch` trait + `MAX_CPUS/CpuId` + 把 `Scheduler` 拆 per-CPU（单核下零开销回退）。
2. **Step 2  Cortex-A9 最小骨架**：`ports/cortex-a` 新建，boot → EL1 + PL011 + panic，`qemu-system-arm -machine vexpress-a9` 跑 hello world。
3. **Step 3  MMU L1 section**：建临时 identity 页表 + map/unmap/tlbi，单核 + 关闭 SMP。
4. **Step 4  真进程模型**：`Process` 抽象 + `Kernel.processes` 替换全局 `cspace`。
5. **Step 5  Cap ↔ MMU 同步**：`MapMemory/UnmapMemory/RegionGrant` syscall + 安全位 `cross_namespace/share_physical/mmio_device`。
6. **Step 6  TicketLock 升级**：`caplet-sync` 改 ticket lock，单核 compare_exchange 回退。
7. **Step 7  SMP 启动**：PSCI `cpu_on` + `secondary_main` + GIC SGI 通路跑通。
8. **Step 8  SmpScheduler 完整化**：per-CPU run queue + 100-tick 负载均衡 + IPI reschedule。
9. **Step 9  IPC Namespace**：`IpcNamespace` + 5 个跨 namespace syscall + `Cap<Endpoint>` 派生扩展。
10. **Step 10  Loader 骨架**：新建 `crates/caplet-loader/`，`ModuleLoader` trait + `Allocator` + `Sandbox` 框架。
11. **Step 11  ELF 子集**：32-bit ARM ELF + 仅 `PT_LOAD` + `R_ARM_RELATIVE`，拒绝 `PT_DYNAMIC/PT_INTERP`。
12. **Step 12  .capp 容器**：自定义紧凑二进制布局 + 显式 cap manifest。
13. **Step 13  签名校验**：`ed25519-dalek` + key id allowlist + granted/required 一致性。
14. **Step 14  load_module 完整流程**：alloc→copy→reloc→cspace→process→返回 `Cap<Module>`。
15. **Step 15  hello-dynamic-loader demo**：静态嵌入 `.capp`，验证静态/动态双模式并存。
16. **Step 16  Loader 观测**：`LOADED_COUNT/ACTIVE_MODULES/FAILED_VERIFICATIONS` + audit ring buffer。
17. **Step 17  单元测试**：`loader_smoke.rs` + host `smp_smoke.rs` + kernel syscall 扩展测试。
18. **Step 18  Kani harness**：`loader_manifest / mmu_permissions / smp_ipc`。
19. **Step 19  CI 流水线**：`build-loader / build-cortex-a9 / qemu-cortex-a9 / qemu-smp-smoke`。
20. **Step 20  bench-harness 扩展**：`loader_throughput / smp_ipc_latency / mmu_map_unmap`。
21. **Step 21  文档**：ADR 0010–0013 + 更新 README / dev-quickstart / security-model。
22. **Step 22  最终验收**：`cargo test --workspace` + `cargo clippy -- -D warnings` + 验收清单逐项勾选 + README 状态表更新。

> 并行加速：Step 9（IPC namespace 协议）与 Step 7–8（调度）只要共享 `TicketLock`（Step 6）即可并行推进；Step 11 与 Step 12 也可分别交给两位工程师独立做。其它步骤**禁止并行**，否则会陷入合并地狱。

### 每步门禁（所有步骤通用）

每完成一个 Step 必须：
- `cargo test --workspace` 全绿
- `cargo clippy --all-targets -- -D warnings` 全绿
- 相关 QEMU smoke test 通过
- 更新 `README.md` 状态表对应行
- 在 frontmatter 把对应 todo 标 `completed`

## 项目背景

`docv1.md` §12 中 M3 的原始定义：
- 静态链接 + 动态加载（ELF / 自定义容器格式）双模式
- Cortex-A53 支持（MMU，真进程隔离）
- 多核调度（SMP）
- 跨能力命名空间 IPC

当前状态（M1+M2 已完成）：
- `crates/caplet-cap/` 已实现 CSpace、双轨 epoch/parent-tree 撤销、Handle/Cap/CapSet、权限位、安全位
- `crates/caplet-kernel/` 已实现静态优先级调度器、ready_queue、单核调度框架、syscall 分发
- `ports/cortex-m/` 已实现 Cortex-M3/M4 端口、上下文切换、PendSV、tickless
- `crates/caplet-async/`、`caplet-task/`、`caplet-sync/` 已实现 stackless async + 同步原语
- `crates/caplet-fs/`、`caplet-net/`、`caplet-security/` 已有能力化抽象
- `boards/hello-qemu-cm4`、`boards/industrial-demo` 已有最小 demo

M3 缺失项：
- 无 `ports/cortex-a/` 端口、无 MMU/EL1 抽象
- 调度器为单核（`Scheduler` 没有 per-CPU run queue）
- 无动态加载器（`caplet-loader` crate 缺失）
- 无 ELF 解析器、无 `.capp` 容器格式
- 无跨 CSpace IPC 能力（当前仅同 task 视图）
- CI 只跑了 `mps2-an385` / `mps2-an500`，无 `vexpress-a9` / `realview-eb-mpcore`

---

## Step 1（Phase 1）: 内核抽象层扩展（M3 基础铺垫）

### 1.1 抽取架构无关的 `Arch` trait

新建 `crates/caplet-kernel/src/arch.rs`，定义：

```rust
pub trait Arch {
    type Context: Copy + Default;
    fn early_init();          // 关中断、清 BSS、栈检查
    fn enable_mmu(cfg: &MmuConfig);   // M3: noop; A9: 建页表
    fn context_switch(prev: *mut Self::Context, next: *const Self::Context);
    fn ipi_send(cpu_id: u32, reason: IpiReason);
    fn fence();                // dmb/dsb 统一抽象
    fn now_ticks() -> u64;
}
```

为 Cortex-M 写 `pub struct CortexMArch;`，为 Cortex-A9 写 `pub struct CortexAArch;`，由 `cfg` 选择。

### 1.2 拆分 `Scheduler` 为 per-CPU

改造 `crates/caplet-kernel/src/scheduler.rs`：
- 把当前单 `Scheduler` 改为 `SmpScheduler { run_queues: [ReadyQueue; MAX_CPUS], current: [Option<u32>; MAX_CPUS], balance: LoadBalancer }`
- 抽取公共逻辑到 `trait LocalScheduler`，原 `Scheduler` 改为 `LocalScheduler` 的别名 + per-cpu 0 默认实例
- 新增 `cpu_count(): u32`、`pick_next_global(cpu) -> ScheduleOutcome`、`migrate(task_id, dst_cpu)` API
- 保留单核行为（`MAX_CPUS=1`）零开销路径

### 1.3 引入 `MAX_CPUS` 常量与 `CpuId` newtype

`crates/caplet-kernel/src/cpu.rs`：
```rust
pub const MAX_CPUS: usize = 4;
pub struct CpuId(pub u32);
pub fn current_cpu() -> CpuId;     // TPIDRRO_EL0 / Cortex-M: const 0
```

修改 `ports/cortex-m/src/context.rs` 的 PendSV 路径，记录当前 PSP 槽位时附带 `CpuId`。
修改 `ports/cortex-m/src/systick.rs`：`SysTick` 触发时写 `CURRENT_CPU`（Cortex-M 单核固定为 0）。

---

## Step 2–3（Phase 2）: 端口层 —— Cortex-A9 / AArch32 port

### 2.1 新建 `ports/cortex-a/`

```
ports/cortex-a/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── boot.rs          # _start, SVC 切换到 EL1
│   ├── context.rs       # task context (r4-r11, lr, spsr)
│   ├── switch.S         # context switch
│   ├── mmu.rs           # L1 page table (4KB granule, 1MB section)
│   ├── gic.rs           # GICv2 (CPU interface + distributor)
│   ├── smc.rs           # SMP spin table / PSCI entry
│   ├── panic.rs
│   ├── kernel_bridge.rs
│   ├── uart_pl011.rs    # vexpress-a9 PL011 UART
│   └── tick.rs          # generic timer / SP804
└── cortex-a9.ld         # linker script (1MB section aligned)
```

### 2.2 `ports/cortex-a/src/boot.rs`

启动流程：
1. SVC 入口：检查当前 EL，进入 EL1（`cps #0x16` 之前 `msr spsr_cxsf, ...`）
2. 关所有中断（GICv2 distributor + CPU interface）
3. 清 BSS、复制 .data
4. 建临时 1:1 L1 页表（identity map kernel + devices），打开 MMU + cache
5. 跳到 `rust_main`

### 2.3 `ports/cortex-a/src/mmu.rs`

```rust
pub struct MmuConfig {
    pub kernel_vaddr: u32,       // 0x4000_0000
    pub kernel_paddr: u32,       // 0x4000_0000
    pub modules_pool_paddr: u32, // 0x6000_0000, 16MB
    pub modules_pool_size: usize,
    pub ipc_shm_paddr: u32,      // 0x7000_0000, 4MB
}

pub fn build_l1(cfg: &MmuConfig) -> L1Table;   // 4096 entries × 4 bytes
pub fn map_module(table: &mut L1Table, vaddr: u32, paddr: u32, size: usize, perms: PagePerms);
pub fn unmap_module(table: &mut L1Table, vaddr: u32, size: usize);
pub fn enable_mmu(l1: &L1Table);   // TTBR0 + DACR
pub fn tlb_invalidate_vaddr(vaddr: u32);
```

实现细节：
- 用 1MB section descriptor（短表），不做 2 级页表（M3 阶段够用）
- 用户态页：AP=0b011（kernel RW, user none）
- 内核页：AP=0b001（kernel RW）
- 设备页：AP=0b001 + TEX=0b000 + C=0 + B=1（strongly-ordered）
- 撤销 `Cap<MemoryRegion>` 时同步改对应 L1 entry + `tlbi va, rs`

### 2.4 `ports/cortex-a/src/context.rs`

`TaskContext` 字段：`r4..r11, sp, lr, cpsr`
- 与 Cortex-M 不同：AArch32 task 退出靠 `eret`，`cpsr` 必须保存
- `__caplet_switch(old, new)` 改为基于 SVC 的协作切换 + 基于 IRQ/FIQ 的抢占切换
- PendSV-like 异常：SVC #0x10（系统调度请求）

### 2.5 `ports/cortex-a/src/gic.rs`

GICv2 抽象：
- `gic_init()`：写 GICD_CTLR、GICC_CTLR、GICC_PMR
- `gic_enable_irq(id, priority)`
- `gic_ack() -> Option<u32>`、`gic_eoi(id)`
- `gic_send_sgi(cpu_mask, sgi_id)` —— IPI 用于 SMP 调度

### 2.6 `ports/cortex-a/src/smc.rs`

PSCI（Power State Coordination Interface）引导：
- `psci_cpu_on(cpu_id, entry)` 通过 `smc #0` 调 ATF 或 PSCI
- QEMU vexpress-a9 默认开启 PSCI，无需手写 spin table

---

## Step 4–5（Phase 3）: 真进程模型（Capability = 物理页一致）

### 3.1 进程抽象

新建 `crates/caplet-kernel/src/process.rs`：
```rust
pub struct Process {
    pub pid: u32,
    pub parent: Option<u32>,
    pub state: ProcessState,   // Active, Suspended, Terminated
    pub cspace: CSpace,        // 私有 CSpace（替换当前的 SHARED_CSPACE_PTR）
    pub page_table: L1Handle,  // MMU L1 table id (Cortex-M 用 None)
    pub ipc_endpoints: BTreeMap<EndpointId, Endpoint>,
    pub ipc_namespace: NamespaceId,
    pub budget: ResourceBudget,
    pub entry: u32,
    pub stack_base: u32,
    pub stack_size: u32,
}
```

### 3.2 改造 `Kernel.cspace`

`crates/caplet-kernel/src/kernel.rs`：
- 删除全局 `cspace: CSpace`
- 新增 `processes: BTreeMap<u32, Process>`
- `with_current_cspace` 改为根据 `CURRENT_CPU -> current_pid` 查 `processes[pid].cspace`
- 删除 `SHARED_CSPACE_PTR` fallback

### 3.3 `Cap<MemoryRegion>` ↔ MMU 页表同步

`crates/caplet-kernel/src/syscall_cap.rs` 新增 syscall：

```rust
Syscall::MapMemory     // 把 Cap<MemoryRegion> 映射到当前进程虚拟地址
Syscall::UnmapMemory
Syscall::RegionGrant   // 把 Cap<MemoryRegion> 转交给另一个进程
```

实现：
1. 校验当前进程持有源 `Cap<MemoryRegion>`、目标 PID 持有目标 slot
2. `mmu::map_module(l1, vaddr, paddr, size, perms)`
3. 在目标进程 `cspace` 中插入一个指向同一物理页的 `Cap<MemoryRegion>`（带"共享"安全位）

撤销时：
1. `mmu::unmap_module(l1, vaddr, size)`
2. `mmu::tlb_invalidate_vaddr(vaddr)`（IPI 给目标 CPU）

### 3.4 安全位收紧

`crates/caplet-cap/src/safety_bits.rs` 新增位：
- `cross_namespace: bool` —— 跨 IPC 命名空间授权
- `share_physical: bool` —— 是否指向共享物理页
- `mmio_device: bool` —— 是否设备寄存器（强序访问）

派生时这些位只能收紧（与现有 `audit/infect/persistent/transferable` 规则一致）。

---

## Step 6–8（Phase 4）: 多核调度（SMP）

### 4.1 启动多核

`crates/caplet-kernel/src/smp.rs`：
```rust
pub fn boot_secondary_cpus(primary_entry: fn() -> !);   // 通过 PSCI cpu_on
pub fn secondary_main();   // 每个 secondary CPU 跑这个
```

`secondary_main()` 流程：
1. 启用 MMU（同一份 L1 表）
2. 启用 GIC CPU interface
3. 自旋等待 `START_SECONDARY` 信号
4. 进入本地 `run_queue.schedule()` 循环

### 4.2 调度策略

`crates/caplet-kernel/src/scheduler.rs` 改造为 per-CPU run queue + 全局负载均衡：

```rust
pub struct SmpScheduler {
    run_queues: [SpinLock<ReadyQueue>; MAX_CPUS],
    current: [AtomicU32; MAX_CPUS],
    idle_tasks: [u32; MAX_CPUS],
    affinity: BTreeMap<TaskId, AffinityMask>,
    global_runqueue: SpinLock<ReadyQueue>,   // 溢出队列
}
```

调度算法：
- **任务放置**：初始 spawn 时按 `affinity` 分配；否则 `run_queue.len() %` 选最少
- **抢占**：`scheduler_tick(cpu)` 每 tick 调用；如本地 `run_queue` 空则从 `global_runqueue` 拉一批
- **负载均衡**：每 100 ticks 各 CPU 检查邻居队列长度差 > 阈值，迁移一个任务
- **跨核 IPI**：GIC SGI #0 = "reschedule"，目标 CPU 在 IRQ 入口触发 `schedule()`

### 4.3 跨核同步原语

`crates/caplet-sync/src/spin.rs` 已有自旋锁，需要做 ticket lock 化（避免单 CPU 饿死）：
- 替换为 `TicketLock<T>` 内部 `AtomicU32 next + serving`
- Cortex-M 单核下退化为 `compare_exchange`

`crates/caplet-sync/src/mutex.rs` 保持 `SpinLock` API，底层替换。

### 4.4 IPI 处理

`ports/cortex-a/src/gic.rs` 新增 SGI 处理：
- SGI #0：调度请求，目标 CPU `trigger_context_switch()`
- SGI #1：TLB shootdown，附带 vaddr
- SGI #2：halt 请求

---

## Step 9（Phase 5）: 跨能力命名空间 IPC

### 5.1 Namespace 抽象

`crates/caplet-kernel/src/ipc_namespace.rs`：
```rust
pub struct IpcNamespace {
    pub id: u32,
    pub endpoints: BTreeMap<EndpointName, EndpointHandle>,
    pub allowed_publishers: CapabilitySet,
    pub allowed_subscribers: CapabilitySet,
}

pub fn create_namespace(name: &str) -> NamespaceId;
pub fn join_namespace(pid: u32, ns: NamespaceId, capability: Capability);  // 需 capability
```

### 5.2 扩展 IPC 协议

`crates/caplet-kernel/src/syscall_ipc.rs` 新增 syscall：

```rust
Syscall::NsCreate      = 60  // 创建 namespace（需 Cap<NamespaceRoot>）
Syscall::NsJoin        = 61  // 加入 namespace
Syscall::NsPublish     = 62  // 跨 namespace 发布消息
Syscall::NsSubscribe   = 63  // 跨 namespace 订阅
Syscall::NsEndpoint    = 64  // 创建 namespace-scoped endpoint
```

端点路由：
1. 客户端发消息时指定 `endpoint_name`
2. 内核在客户端所属 namespace 内查 endpoint
3. 命中 → 投递；未命中 → 沿 namespace 父链向上查
4. 跨 namespace 投递时检查 `allowed_publishers` capability

### 5.3 `Cap<Endpoint>` 派生

`crates/caplet-cap/src/derivation.rs` 扩展 `derive` 支持：
- 限制 endpoint 的 namespace 范围
- 限速：`max_msg_per_sec`
- 单次端点（用完即销毁，自动 revocation）

---

## Step 10–15（Phase 6）: 动态加载器 —— 双模式并存

### 6.1 新建 `crates/caplet-loader/`

```
crates/caplet-loader/
├── Cargo.toml
├── src/
│   ├── lib.rs           # ModuleLoader trait + 入口
│   ├── manifest.rs      # ModuleManifest 结构
│   ├── elf.rs           # ELF 简化子集解析器
│   ├── capp.rs          # 自定义 .capp 容器
│   ├── reloc.rs         # 重定位（仅相对地址 + GOT）
│   ├── verifier.rs      # 签名校验
│   ├── sandbox.rs       # 加载时 capability 注入
│   └── allocator.rs     # 模块内存分配（从 modules_pool 取）
└── tests/loader_smoke.rs
```

### 6.2 `caplet-loader/src/manifest.rs`

```rust
pub struct ModuleManifest {
    pub magic: [u8; 4],            // "CAPP" 或 "ELFM"
    pub version: u16,              // manifest 格式版本
    pub name: [u8; 32],            // 模块名
    pub required_caps: Vec<CapDescriptor>,    // 必需能力
    pub granted_caps: Vec<CapDescriptor>,     // 加载器授予
    pub entry_point: u32,
    pub stack_size: u32,
    pub priority: u8,
    pub text_base: u32,
    pub text_size: u32,
    pub data_size: u32,
    pub signature: [u8; 64],       // Ed25519
    pub public_key_id: u32,        // 指向 CA key
}
```

### 6.3 ELF 子集解析器

`caplet-loader/src/elf.rs`：
- 只解析 32-bit ARM ELF（ET_EXEC、EM_ARM）
- 仅支持 program headers（PT_LOAD）；忽略 section headers
- 仅处理 RELATIVE 重定位（R_ARM_RELATIVE）；其他 R_TYPE 返回错误
- 拒绝 dynamic linking（PT_INTERP、PT_DYNAMIC 不允许）
- 输出 `ModuleManifest` + 重定位项列表

复用 [`goblin`](https://crates.io/crates/goblin) crate 的 ELF 解析，包装一个"白名单子集"。

### 6.4 自定义 `.capp` 容器

`caplet-loader/src/capp.rs`：

二进制布局（v1）：
```
| magic (4B) "CAPP" | version (2B) | hdr_size (2B) |
| entry_point (4B) | stack_size (4B) | priority (1B) | flags (1B) | _pad (2B) |
| text_off (4B) | text_size (4B) |
| data_off (4B) | data_size (4B) |
| reloc_off (4B) | reloc_count (4B) |
| cap_manifest_off (4B) | cap_manifest_count (4B) |
| signature (64B) |
| --- TEXT SECTION --- |
| --- DATA SECTION --- |
| --- RELOC TABLE --- (each 8B: vaddr u32, addend u32) |
| --- CAP MANIFEST --- (each: cap_id [u8; 16], perms u8, safety u8) |
```

设计理由：
- 紧凑：比 ELF header 小 ~10 倍
- 不依赖 dynamic linker
- 显式 cap manifest，便于审计

### 6.5 签名校验

`caplet-loader/src/verifier.rs`：
- 引导时把根公钥烧入 `KernelConfig.signing_keys`
- 用 [`ed25519-dalek`](https://crates.io/crates/ed25519-dalek) 验签
- 拒绝 key id 不在 allowlist 的模块
- 拒绝 manifest 声明的 `granted_caps` 与实际 `required_caps` 不匹配的模块

### 6.6 加载流程（sandbox.rs）

```rust
pub fn load_module(
    bytes: &[u8],
    loader_caps: &CapabilitySet,
) -> Result<Cap<Module>, LoaderError> {
    let (manifest, image) = parse_container(bytes)?;
    verifier::verify_signature(&manifest, &image)?;
    
    // 1. 分配物理页
    let (vaddr, paddr) = allocator::alloc_module(&manifest)?;
    
    // 2. 复制 .text + .data
    copy_to_memory(paddr, &image.text, &image.data);
    
    // 3. 应用重定位
    reloc::apply(&image.relocs, vaddr)?;
    
    // 4. 建立 CSpace + 注入能力
    let module_cspace = CSpace::new();
    for cap in &manifest.granted_caps {
        if loader_caps.contains(cap) {
            module_cspace.allocate(...)?;
        }
    }
    
    // 5. 创建 Process
    let pid = kernel.create_process(Process::from_manifest(&manifest, module_cspace))?;
    
    // 6. 在当前进程的 CSpace 中插入 Cap<Module>
    let cap = current_cspace().allocate(...)?;
    Ok(cap)
}
```

### 6.7 静态模式并存

docv1 §17.2：
- M3 阶段保留 `boards/industrial-demo` 作为静态模式示例
- 新增 `boards/hello-dynamic-loader/`：启动后从内置字节数组加载一个模块并调用其 `entry()`

---

## Step 16（Phase 7）: 调试与观测

### 7.1 模块加载计数

`crates/caplet-loader/src/lib.rs` 暴露：
```rust
pub static LOADED_COUNT: AtomicU32;
pub static ACTIVE_MODULES: AtomicU32;
pub static FAILED_VERIFICATIONS: AtomicU32;
```

通过 `caplet::service("metrics:loader")` 导出。

### 7.2 模块加载 audit log

`crates/caplet-cap/src/safety_bits.rs` 的 `audit` 位扩展到 module 加载：
- 每次 `load_module` 调用产生一条 audit 记录：pid、模块名、key_id、granted caps
- 落 ring buffer（可选，默认关）

---

## Step 17–20（Phase 8）: 测试与 CI 扩展

### 8.1 新增 QEMU 板

`boards/hello-dynamic-loader/`：
- 启动后从 `include_bytes!("sample_module.capp")` 加载一个静态嵌入的 .capp
- 模块导出 `fn entry(caps: CapSet)`，demo 中打印 "hello from module\n"

`tests/qemu-cortex-a9/run.sh`：
```bash
qemu-system-arm -machine vexpress-a9 -cpu cortex-a9 -smp 2 \
  -m 512M -nographic \
  -semihosting-config enable=on,target=cli \
  -kernel boards/hello-dynamic-loader/target/aarch64.../*.elf
sleep 3
kill $PID
grep -q "hello from module" out.txt
```

### 8.2 Loader 单元测试

`crates/caplet-loader/tests/`：
- `loader_smoke.rs`：
  - 解析一个手工构造的合法 ELF，验证 `verify_signature` 失败（无 key）
  - 解析一个手工构造的合法 .capp，验证 `manifest` 字段
  - 重定位单元测试：构造已知输入，验证 addend 计算
  - 不合法 magic 拒绝
  - signature 篡改拒绝

### 8.3 SMP 单元测试

`crates/caplet-kernel/tests/smp_smoke.rs`（host）：
- 模拟 4 个逻辑 CPU，验证 `SmpScheduler.schedule()` 不会把同一任务给两个 CPU
- 验证 `migrate(task, dst)` 后 `cpu_of(task)` 返回新值

### 8.4 Kani 形式化扩展

`tools/kani/harness/loader_manifest.rs`：
- 验证 `manifest.required ⊆ manifest.granted` 在 `load_module` 入口处成立
- 验证签名长度 = 64 bytes
- 验证 ELF magic 校验拒绝任何非 `0x7f 'E' 'L' 'F'` 输入

`tools/kani/harness/mmu_permissions.rs`：
- 验证 `map_module` 不会同时把同一 vaddr 映射为 user-writable 和 kernel-writable
- 验证 `unmap_module` 后再访问 vaddr 触发 page fault（mock）

`tools/kani/harness/smp_ipc.rs`：
- 验证 IPC 投递的源/目标 PID 必然不同
- 验证 namespace 权限检查对每个 endpoint 都执行

### 8.5 CI 流水线扩展

`.github/workflows/ci.yml` 新增 jobs：

```yaml
build-loader:
  - cargo build -p caplet-loader --target thumbv7em-none-eabihf
  - cargo build -p caplet-loader --target aarch64-unknown-none  (or armv7-a)

build-cortex-a9:
  - rustup target add aarch64-unknown-none-softfloat (or armv7-a)
  - cd boards/hello-cortex-a9 && cargo build --release

qemu-cortex-a9:
  needs: build-cortex-a9
  - apt install qemu-system-arm
  - bash tests/qemu-cortex-a9/run.sh

build-dynamic-demo:
  - cargo build --release --bin hello-dynamic-loader
  - tools/elf2capp.py boards/industrial-demo/target/.../*.elf > sample_module.capp

qemu-smp-smoke:
  - qemu-system-arm -machine vexpress-a9 -smp 4 ...
```

### 8.6 benchmark 扩展

`crates/caplet-bench-harness/src/lib.rs` 新增：
- `loader_throughput`：每秒能验证多少个 64KB 模块（目标：> 200/s host）
- `smp_ipc_latency`：跨 CPU IPC 投递时间（目标：< 5 µs QEMU）
- `mmu_map_unmap`：`map + unmap` 一次 4KB 页时间（目标：< 1 µs QEMU）

---

## Step 21（Phase 9）: 文档

### 9.1 新增 ADR

`docs/adr/0010-cortex-a-port-strategy.md`：
- 用 vexpress-a9 作为 M3 主开发板，realview-eb-mpcore 作为副板
- L1 section descriptor 而非 2-level 页表（v1 简化）
- GICv2 而非 GICv3（兼容性更广）

`docs/adr/0011-loader-container-format.md`：
- ELF 子集作为开发者首选（调试无缝）
- .capp 作为生产部署首选（紧凑 + 显式 cap manifest）
- 加载器 trait 抽象两种格式

`docs/adr/0012-smp-scheduler-design.md`：
- per-CPU run queue + 全局溢出队列
- 100-tick 负载均衡周期
- IPI-based reschedule

`docs/adr/0013-ipc-namespace-model.md`：
- Namespace 树形继承
- Endpoint 跨 namespace 路由
- Capability 控制 join/publish/subscribe

### 9.2 更新现有文档

`docs/dev-quickstart.md`：增加 `cargo run --bin hello-cortex-a9` 章节
`docs/security-model.md`：增加模块签名、namespace 边界描述
`README.md`：更新 M3 阶段状态

---

## 验收标准

| 目标 | 验收条件 |
|---|---|
| 静态模式保持 | `boards/industrial-demo` 在 M2 已通过的 QEMU 输出不变 |
| 动态加载 ELF | `boards/hello-dynamic-loader` 加载一个 `.elf` 模块并执行其 entry |
| 动态加载 .capp | 同上，但模块是手工构造的 `.capp` |
| 签名拒绝 | 修改 sample module 一字节，加载应失败 |
| 重定位 | 模块用 PIC，reloc 表正确加 base address |
| Cortex-A9 启动 | `qemu-system-arm -machine vexpress-a9` 跑通 hello world |
| 真 MMU | 用户态访问未映射地址触发 Data Abort，内核不崩 |
| 真进程隔离 | 两个 process 的页表互不可见 |
| SMP 启动 2 核 | 启动后两个 CPU 都能跑任务，进程计数 = 主核 + 副核之和 |
| 跨 CPU IPC | CPU0 发消息给 CPU1 上的进程，CPU1 收到并回复 |
| 跨 namespace 拒绝 | 无 `cross_namespace` capability 的端点接收不到 |
| CI 通过 | GitHub Actions 全部新增 job 绿 |

---

## 关键文件清单

新建：
- `crates/caplet-kernel/src/{arch,process,smp,ipc_namespace,cpu}.rs`
- `crates/caplet-loader/` （整 crate）
- `ports/cortex-a/` （整 port）
- `boards/hello-cortex-a9/`
- `boards/hello-dynamic-loader/`
- `tools/elf2capp.py`
- `tests/qemu-cortex-a9/run.sh`
- `tests/qemu-smp-smoke/run.sh`
- `docs/adr/0010`～`0013-*.md`

改动：
- `crates/caplet-kernel/src/{lib,scheduler,kernel,syscall,syscall_cap,syscall_ipc}.rs`
- `crates/caplet-cap/src/{safety_bits,derivation,permissions}.rs`
- `crates/caplet-sync/src/{spin,mutex}.rs`
- `Cargo.toml`（workspace + 新依赖）
- `.github/workflows/ci.yml`
- `crates/caplet-bench-harness/src/lib.rs`
- `tools/kani/run_catalogue.sh`
- `tools/grep_gate.allowlist`（如果新增 unsafe 字符串）

---

## 顺序执行总览（Phase → Step 映射）

下面这张表把原 Phase X 拆解到顺序执行的 Step，每个 Step 都不能跳过上一个：

| 原 Phase | 对应 Step            | 关键交付物                                                                 |
|----------|----------------------|----------------------------------------------------------------------------|
| Phase 1  | Step 1               | `arch.rs` / `cpu.rs` / per-CPU `Scheduler` 抽象                              |
| Phase 2  | Step 2, Step 3       | `ports/cortex-a/` 骨架 → MMU L1 section 表                                 |
| Phase 3  | Step 4, Step 5       | `Process` 抽象 + `Cap<MemoryRegion> ↔ MMU` 同步 syscall                    |
| Phase 4  | Step 6, Step 7, Step 8 | `TicketLock` → SMP 启动 → `SmpScheduler` 完整化                            |
| Phase 5  | Step 9               | `IpcNamespace` + 跨 namespace syscall + Endpoint 派生                     |
| Phase 6  | Step 10, 11, 12, 13, 14, 15 | `caplet-loader/` 骨架 → ELF 子集 → `.capp` → 签名 → load_module → demo |
| Phase 7  | Step 16              | Loader metrics + audit log                                                 |
| Phase 8  | Step 17, 18, 19, 20  | 单元测试 + Kani + CI + bench                                               |
| Phase 9  | Step 21              | ADR 0010–0013 + README/quickstart/security-model 更新                       |
| （收尾） | Step 22              | 最终验收门禁                                                                |

### 顺序执行规则（避免阻塞）

1. **Step 1** 先做抽象层；现有 cortex-m port + scheduler 的所有测试必须保持绿。
2. **Step 2 → Step 3** 单步推进，每步 QEMU vexpress-a9 跑通 hello world；不强求 SMP。
3. **Step 4 → Step 5** 在 Cortex-A 进程模型下用单核验证 MMU；通过 Kani。
4. **Step 6 → Step 7 → Step 8** SMP 调度器在 host 上单测通过，再上 QEMU SMP。
5. **Step 9** IPC namespace 与 Step 7–8 部分并行（只要 Step 6 已落地）。
6. **Step 10 → Step 11 → Step 12** 先做 ELF 子集（开发者最常用），后做 .capp。
7. **Step 13–15** 串行依赖 Step 12；Step 15 必须能跑通才算 loader 路径打通。
8. **Step 16–22** 在每个对应 Step 完成后增量追加。

### 每完成一个 Step 必须

- `cargo test --workspace` 全绿
- `cargo clippy --all-targets -- -D warnings` 全绿
- 相关 QEMU smoke test 通过
- 更新 `README.md` 状态表对应行
- 把 frontmatter 对应 todo 标 `completed`