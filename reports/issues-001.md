# CapletRT 项目独立代码审计报告（issues-001）

- **审计时间**：本轮会话（工作区 `I:\CapletRT`，HEAD `7f1c0e8` + 大量未提交改动）
- **审计方式**：独立、可复现。所有结论均经**实际编译 / 实际运行 / 实际读码**三方交叉验证；未经验证的推断均标注【推断】。
- **审计范围**：`crates/`（23 个 crate）、`ports/`（cortex-m / cortex-a / riscv / allwinner）、`boards/`（12 个板级 crate）、`tests/`、`tools/`、`docs/`、`.github/workflows/ci.yml`
- **结论（一句话）**：**问题没有全部解决。**“hello world 能打印”是真实的，但**内核从未接管过调度**——所有板子跑的是一个单线程 async 执行器 + 一个完全不参与调度的 `Scheduler` 对象。此外 STM32 全系板、RISC-V 板、3 个工业 demo 板**编译失败**，`--all-features` 与 `cargo bench` 在 CI 中**必然失败**。

---

## 0. 执行摘要

### 0.1 一句话结论

项目目前处于 **“演示可用、内核不可用”** 的状态：镜像能启动、日志能打印、单任务能跑；但**抢占式调度、上下文切换、系统调用入口、进程隔离、MMU、持久化文件系统、网络收发、OTA 校验、模块签名**这些“RTOS 内核”的核心能力**全部没有真正接通**，其中相当一部分是显式的空实现（返回 `Ok(())` / `true` / `0`），另有若干**可证明为错误**的实现。

### 0.2 严重度分布

| 级别 | 含义 | 数量 |
|---|---|---|
| **P0** | 产品根本不具备其声称的能力，或构建/CI 必败，或安全属性被虚假满足 | **14** |
| **P1** | 功能性缺陷 / 架构性错误，会导致误判、维护困难或运行时错误 | **24** |
| **P2** | 代码质量、一致性、文档失真 | **17** |
| 合计 | | **55** |

### 0.3 本次审计中**实测**到的事实（可复现）

| # | 实测命令 | 实测结果 |
|---|---|---|
| 1 | `cargo test --workspace` | **失败**：`caplet-hal-stm32f4` 47 个 `missing_docs` 错误 |
| 2 | `cargo test --workspace --exclude caplet-hal-stm32f4 …` | **成功**：约 270 个测试全绿（但这些测试覆盖的是“假的”路径，见事实 11） |
| 3 | `cargo test --workspace --all-features` | **失败**：`caplet-log` 4 个错误（`E0277` ×2、`E0596` ×2）；`caplet-loader --features crypto` 6 个错误 |
| 4 | 干净目录构建 `hello-qemu-cm4`（`thumbv7em-none-eabihf`） | **失败**：`managed 0.8` 需要 `std`；`caplet-timer` 需要 `AtomicU64`（`E0432`） |
| 5 | 同样方式构建 `hello-nucleo-f446ze-qemu` / `nucleo-f446ze` | **失败**：`caplet-hal-stm32f4` 47 个 `missing_docs` |
| 6 | 同样方式构建 `industrial-demo` | **失败**：3 个 `E0599/E0609`（`MockSensor::read` 不存在、`Reading.device_id` 不存在、`f32::fract` 不存在） |
| 7 | 同样方式构建 `medical-device` / `automotive` / `energy-meter` | **失败**：`E0152 duplicate lang item panic_impl` |
| 8 | 同样方式构建 `hello-riscv`（`riscv32imac-unknown-none-elf`） | **失败**：`E0463` 找不到 `std`、无 `#[global_allocator]`、无 `#[panic_handler]`、无 `main` |
| 9 | QEMU 实跑 `hello-qemu-cm4`（mps2-an500） | **成功**：打印 `hello world`；但 `control` 计数器一路涨到 34 000+，`network_task` **一次都没有输出** → 证明没有抢占 |
| 10 | QEMU 实跑 `hello-allwinner-a20-qemu`（vexpress-a9） | **成功**：输出 `Hello from Allwinner A20` —— 但这是**纯 `global_asm` + `svc #0x123456`**，**一行 Rust 内核代码都没跑**（见 P0-02） |
| 11 | `llvm-nm` 检查 `hello-qemu-cm4` 镜像 | **`__caplet_switch`、`__caplet_start_first` 符号完全不存在** → 上下文切换代码被链接器整体剔除 |
| 12 | `cargo tree -p caplet-cap -e features` | `hashbrown` 默认启用 `ahash`；裸机环境下哈希种子来源不可控【推断】 |

---

## 1. 模块完成度总表

> **统计口径**：完成度 = （真实实现的功能点）/（README/ADR 承诺的功能点），对“假实现”记 0 分。与“测试是否通过”无关。

| 模块 | 代码行数 | 单元测试 | 完成度 | 状态判定 |
|---|---:|---:|---:|---|
| `caplet-macros` / `macros-exports` | 201 + ~150 | 0 | **45%** | 宏生成的 `#[task]` 只是普通 async fn，优先级 / 栈 / 名字全部丢失 |
| `caplet-cap`（能力模型） | 2180 | 12 | **40%** | 数据结构齐全，但 `Cap::get()` 永远返回 `CapError::Invalid`；句柄无 generation 校验；`CapGrant` 空实现 |
| `caplet-task` | 759 | 2 | **35%** | `TaskContext::new` 丢弃 `arg`、`sp` 未使用；TCB 与内核 TCB 是两套互不相干的结构 |
| `caplet-async` | 901 | 8 | **55%** | 单线程执行器**能跑**；但持锁 poll（潜在自死锁）、无公平性、无优先级、`priority` 字段被忽略 |
| `caplet-sync` | 973 | 8 | **30%** | 全部自旋锁；`Mutex::lock` 从不阻塞；无优先级继承；`Semaphore::acquire` 无限自旋 |
| `caplet-kernel`（调度/内核核心） | 5710 | 56 | **20%** | `Kernel::start()` 是纯空转 loop；`current_cpu()` 恒为 0；`find_pid_for_task` 返回“第一个进程”；`make_ready` 无任务表 |
| `caplet-kernel`（系统调用） | （含上） | （含上） | **15%** | **没有任何端口把 `SVC` 接到 `syscall_dispatch`**；`CapGrant`/`TimerSet`/`FlashToken`/`CryptoToken`/`NS*`/`Module*` 全是空实现 |
| `ports/cortex-m` | 799 | 1 | **25%** | `__caplet_switch` 汇编基本必定寄存器冲突；`pend_pendsv` 写错偏移且关中断；`boot.rs` 全是空函数 |
| `ports/cortex-a` | 849 | 5 | **15%** | 无 `MmuOps` 实现；`context_switch` 是 `loop {}`；无 GIC/SGI 真正接入 |
| `ports/riscv` | 1319 | 1 | **10%** | 板子根本编译不过；`enable_mmu` 空实现 |
| `boards/hello-allwinner-a20-qemu` | 121 | 0 | **5%** | 只是裸汇编打印一句话，**不链接内核** |
| `boards/hello-qemu-cm3` / `cm4` / `cm4-fs` | — | — | **60%** | **真能跑**（干净构建 + QEMU 实跑通过），但内核未参与调度 |
| `boards/nucleo-f446ze` / `hello-nucleo-f446ze-qemu` | — | — | **10%** | **编译失败**；且只做 GPIO 闪灯，不链接内核 |
| `boards/industrial-demo` / `medical-device` / `automotive` / `energy-meter` | — | — | **5%** | **全部编译失败**；即使修好也只是“假 demo” |
| `boards/hello-riscv` | 35 | 0 | **5%** | **编译失败** |
| `caplet-fs`（VFS） | 1482 | 16 | **35%** | 只有 RAMFS 真正可用；**LittleFS / FAT32 都是返回 `Unsupported` 的占位符**；syscall 用“路径字符串末 8 字节当句柄”的协议 |
| `caplet-net`（网络） | 1341 | 25 | **20%** | **`Send` 是空实现**；TCP 只有首部解析，无状态机；DHCP 不可用 |
| `caplet-mqttsn` | 543 | 9 | **45%** | 编解码 + 模拟 broker 的 e2e，无真实 socket 绑定 |
| `caplet-loader`（动态加载） | 1864 | 19 | **20%** | ELF 解析**丢掉段布局**；签名校验在默认 feature 下**直接放行**；`load_module` 根本**不拷贝镜像**；`crypto` feature **编译不过** |
| `caplet-security` | 426 | 5 | **20%** | `CryptoOp` 只是枚举；token 检查恒为 `true`；软件 TRNG 是固定种子 LCG |
| `caplet-ota` | 494 | **0** | **5%** | `Verifier` 恒返回 `UnknownKey`；`BootManager` 直接 `write_volatile` 写 flash 且**从不擦除** |
| `caplet-hal` + `-stm32f4` / `-nrf52` / `-gd32` | 761 + 1050 + 632 + ~120 | 2 | **20%** | stm32f4 **编译失败**、gd32 跟随失败、nrf52 用错 trait 名、i2c/spi/led 无实现 |
| `caplet-log` | 531 | 0 | **40%** | 只支持 semihosting 一种 writer；`--all-features` 编译失败；tag 位图方案会静默丢 writer |
| `caplet-timer` | 230 | 8 | **15%** | TimerWheel **未接入内核**；裸机无 `AtomicU64`；时间窗仅 128 tick |
| `caplet-budget` / `caplet-sensors` / `caplet-bench-harness` | 546 / 174 / ~60 | 8 / 3 / 0 | **30% / 50% / 30%** | budget 未接入调度；sensors 是模拟值 |
| `tools/tickless_verify` | 45 | 3 | **20%** | 只验证纯函数，不验证 QEMU 行为 |
| `tools/kani/*`（形式化） | ~1000 | — | **10%** | 未安装 kani；CI 中静默跳过；harness 依赖的是被 mock 的路径 |
| `.github/workflows/ci.yml` | — | — | **15%** | 见 P0-03 |
| `docs/` + `README.md` + `SUMMARY.md` | — | — | **25%** | 里程碑勾选与实际严重不符（见 P0-13） |

**项目整体完成度（以“可作为工业级 RTOS 内核”为 100%）：约 26%**

---

## 2. P0 —— 必须立即处理的问题（14 项）

### P0-01　内核从未参与调度：所有板子实际跑的是单线程执行器

**证据**
- `crates/caplet-macros-exports/src/lib.rs:117-123`：`drive_executor_forever()` 是 `loop { drive_executor(); cpu_relax(); }`。
- 所有 CM3 / CM4 / cm4-fs / industrial 板的 `main()` 最后都调用 `drive_executor_forever()`，**从不调用 `kernel_start()`**。
- QEMU 实测 `hello-qemu-cm4`：`control`（priority 3）从 tick 1000 一路打到 34000+，`network`（priority 4）**一次都没有输出**。

**影响**
- README 声称的“静态优先级调度器，O(1)”“Tickless idle”、`#[caplet::task(priority = 3, stack = 4096)]`（栈尺寸）**全部不生效**。
- `Scheduler`、`ScheduledTask`、`Kernel::start()`、`timer_tick()` 是**死代码**：没有任何板子驱动它们。

**建议**
1. 让 `#[task]` 宏真正注册进 `Kernel`（编译期产出 `TaskDescriptor` 静态表，启动时枚举），把 `drive_executor_forever` 换成 `kernel_start()`。
2. 加守护：任何板子若既不调用 `kernel_start` 也不显式声明 executor-only，则 CI 失败。

---

### P0-02　Cortex-A / Allwinner A20 端口完全未接入内核

**证据**
- `boards/hello-allwinner-a20-qemu/src/main.rs:64-108`：整份程序就是一个 `global_asm!` 块，`svc #0x123456` 打印一句话后死循环；`Cargo.toml` 虽列了 `caplet-cortex-a`，但代码**没有引用任何 caplet crate**。
- `ports/cortex-a/src/kernel_bridge.rs:39-42`：`install_mmu()` 返回 `static mut MMU: Mmu`，而 `ports/cortex-a/src/mmu.rs:204-210` 中 `context_switch` / `start_first_task` 的实现是 **`loop {}`**。
- `ports/cortex-a` 中**没有任何 `MmuOps` 实现**（全仓库 grep：`MmuOps` 只出现在 `caplet-kernel/src/mmu.rs` 与一处自测）。

**影响**
- README 中 M3 的 Step 1–22 全部勾选（含“SMP 启动（PSCI + secondary_main + GIC SGI）”“真进程模型”“Cap<MemoryRegion> ↔ MMU 同步 syscall”），**与代码事实完全不符**。
- 实测 A20 板的输出是“真实”的，但它证明不了内核任何能力。

**建议**：把该板的验收标准改为“输出 `caplet_kernel_entry` 启动日志 + 至少两个任务交替”，否则应标记为“未接入”。

---

### P0-03　CI 配置必然失败 / 形同虚设（多处）

**证据**（`.github/workflows/ci.yml`）

| 行 | 问题 | 后果 |
|---|---|---|
| 21, 38, 91 … | `uses: dtolnay/rust-action@stable` —— 正确名称是 `dtolnay/rust-toolchain@stable` | 所有 job 第一步即失败 |
| 43 | `cargo clippy --all-targets --all-features -- -D warnings` | 本机实测 `--all-features` 在 `caplet-log` 上就有 4 个编译错误；kernel 50 / riscv 40 / cortex-a 17 个 warning 全变错误 → **必红** |
| 96 | `cargo bench --no-run --workspace …` | 本机实测 `cargo test --workspace` 已因 `caplet-hal-stm32f4` 失败，bench 必然同样失败 |
| 118-121 | `cargo install kani-verifier || true` 后 `bash tools/kani/run_catalogue.sh` | 形式化验证被静默跳过 |
| 76 | `cargo deny check … || true` + `continue-on-error: true` | 许可证 / 漏洞门禁**永久失效** |
| 133-144 | `build-cm3` 只装了 `thumbv6m` target，却把产物路径写死为 `target/thumbv6m-none-eabi/release/hello-qemu-cm3.elf` | 实际产物在 `thumbv7m-none-eabi` 且**无 `.elf` 后缀**，upload 的 artifact 为空 |
| 205-220 | `qemu-f446ze` 用 `|| true` + `echo` | **永远通过**，不校验任何输出 |
| — | 全流程**没有** `cargo test --workspace`、**没有** Renode、**没有** A20 测试 | 与 README/SUMMARY 声称严重不符 |

**建议**：`rust-action` → `rust-toolchain`；拆分 clippy 门禁；QEMU 测试改为 grep 断言并据此返回退出码；补 `cargo test --workspace`。

---

### P0-04　STM32F4 全系编译失败，导致 2 个板子 + 1 个 crate + workspace 测试全部不可用

**证据**
- `crates/caplet-hal-stm32f4/src/lib.rs:17` `#![deny(missing_docs)]`，而 `src/raw.rs` 有 47 处未文档化的 `pub struct` / 字段。
- 实测：`cargo test --workspace` ⇒ `could not compile caplet-hal-stm32f4 (lib) due to 47 previous errors`。
- 连带：`caplet-hal-gd32`（`pub use caplet_hal_stm32f4 as f4_compat`）失败；`nucleo-f446ze`、`hello-nucleo-f446ze-qemu` 失败。

**影响**：**“STM32 板环境全功能”这一目标目前是 0%。**

**建议**：`raw.rs` 的寄存器块用 `#[allow(missing_docs)]`（寄存器名自解释），公共 API 补文档；CI 增加 `cargo build -p caplet-hal-stm32f4` 门禁防止回归。

---

### P0-05　Cortex-M 上下文切换汇编基本必挂，且从未被调用

**证据**（`ports/cortex-m/src/context.rs:96-122`）

```rust
pub unsafe extern "C" fn __caplet_switch(old_sp: *mut u32, new_sp: *mut u32) {
    core::arch::asm!(
        "push {{r4-r11}}",
        "str sp, [{r0}]",      // <- 问题 1
        "mov sp, {r1}",
        "pop {{r4-r11}}",
        "bx lr",               // <- 问题 2
        r0 = in(reg) old_sp,
        r1 = in(reg) new_sp,
        options(noreturn)
    );
}
```

1. `r0` / `r1` 使用 `in(reg)` 约束，编译器**可任意选寄存器**（常常是 `r4`/`r5`）。`push {r4-r11}` 会先把它们压栈，若 `r0` 被选为 `r4`，`str sp, [r0]` 读到旧值 → **写错地址**。
2. `bx lr`：`lr`（r14）**不在 `r4-r11` 内**，`push/pop` 不保存返回地址 → 函数返回时 `lr` 已被覆盖 → **跳转到未定义地址**。
3. `sp` 在 AAPCS 下由编译器管理，保存的 `sp` 与恢复时可能不一致。

**更强的证据**：`llvm-nm` 检查实际编译出的 `hello-qemu-cm4` 镜像，**`__caplet_switch` 与 `__caplet_start_first` 两个符号根本不存在**（被 LTO 剔除）。即：**从未被任何代码调用、从未被任何测试执行**（唯一的测试只验证 `build_initial_stack` 的内存布局）。

**建议**：改用 `global_asm!` 手写并固定寄存器；或直接复用 `cortex-m-rt` 的 `PendSV` + `cortex-m` crate 的异常机制。补 QEMU 集成测试：两任务各自累加共享计数器，断言两者都增长。

---

### P0-06　系统调用入口在真实硬件上不存在

**证据**
- 全仓库 grep `syscall_dispatch`：只出现在 `crates/caplet-kernel/src/syscall_dispatch.rs`（定义）、`tools/kani/harness/`、`crates/caplet-kernel/tests/syscall_smoke.rs`（host 测试）。
- **`ports/cortex-m` / `cortex-a` / `riscv` 中没有任何 `SVCall` handler**。`ports/cortex-m/src/interrupt.rs` 只定义了 `Exception` 枚举与 `ExceptionFrame` 结构。
- `syscall_smoke.rs` 的 10 个测试全部是**直接调用 Rust 函数**，**没有一次跨过 SVC 边界**。

**影响**：整个能力模型的“安全边界”不存在。用户代码一旦运行在 Thread 模式，没有 SVC 入口就没有权利检查，`CapDerive` 等一切“内核把关”均为纸面承诺。

**建议**：第一时间接上 `SVCall` handler（从 `ExceptionFrame` 取 r0-r3 + 立即数作为 syscall 号），并写板级测试。

---

### P0-07　PendSV 触发函数写错偏移 + 关中断

**证据**（`ports/cortex-m/src/context.rs:192-207`）

```rust
"ldr r0, =0xE000ED04",      // ICSR
"movs r1, #0x10000000",     // PENDSVSET
"str r1, [r0, #4]",         // <- 写到了 0xE000ED08 = VTOR！
"cpsid i",                  // <- 函数返回后中断状态未保证恢复
"cpsie i",
```

- `ICSR` 偏移应为 `#0`，`#4` 是 `VTOR`（向量表偏移寄存器）。**这条指令会把向量表基址改成 `0x1000_0000`，随后任何异常都会跳到非法地址。**

**建议**：用 `cortex_m::peripheral::SCB::set_pendsv()`，或修正为 `str r1, [r0]` 并去掉 `cpsid/cpsie`。

---

### P0-08　`SmpScheduler::tick()` 会复制任务，且 `prev` 永远是 0

**证据**（`crates/caplet-kernel/src/smp_scheduler.rs:234-249`）

```rust
Some(next) => {
    let prev_id = self.cpus[cpu.index()].current.take().map(|t| {
        self.cpus[cpu.index()].run_queue.enqueue(t);   // <- 把 current 重新入队
        0                                              // <- 丢弃真正的 prev id
    });
    ...
    SmpOutcome::LocalSwitch { cpu, prev: prev_id.unwrap_or(0), next: next_id }
```

- `prev` 恒为 `0`（注释自承是 “placeholder”），上层无法据此做正确的上下文保存。
- `SmpScheduler` 内还有一个从不被使用的 `single: Scheduler` 字段，与单核调度器**互不通信**。

**建议**：重写为“current 是唯一真相”，出队时**不**把 current 入队；只有 `block_current()` / `make_ready()` 才操作队列。

---

### P0-09　`SmpScheduler::make_ready()` 没有任务表可用

**证据**（`crates/caplet-kernel/src/smp_scheduler.rs:271-399`）

`make_ready(task_id)` 只拿到 id，`requeue_task_on` 调用的 `extract_task` 是**从队列里捞任务**。若任务处于 `current`（正在运行、刚被唤醒），它不在任何队列里，`extract_task` 返回 `None` → `make_ready` **什么都不做**。源码注释称 “Look up the task in the task table is O(n); for M3 we accept this”，但**根本没有表**。

另外 `add_task_with_affinity` 先 `affinity.lock().insert(...)`，随后 `pick_target_cpu` 又直接读 `cpus[...].run_queue.len()`，**没有加锁**，多核下是数据竞争。

**建议**：引入全局 `SpinLock<BTreeMap<u32, ScheduledTask>>` 任务表；`ReadyQueue` 只存 id。

---

### P0-10　**能力系统整体是“装饰性”的**（安全属性被虚假满足）

| 子问题 | 位置 | 说明 |
|---|---|---|
| `Cap<T>::get()` / `get_mut()` 永远返回 `Err(CapError::Invalid)` | `caplet-cap/src/cap.rs:77-107` | 能力**从不真正授予对资源的访问** |
| `Cap::clone()` 不登记派生，`Drop` 是空实现 | `cap.rs:110-127` | 派生关系丢失 → `CapRevoke` 无法追溯 |
| 句柄无 generation 校验 | `handle.rs` + `cspace.rs:103` | `Handle.generation` 恒为默认值，槽位 generation 从不递增 → **ABA 问题**：槽位复用后旧句柄可访问新能力 |
| CSpace 哈希索引是死代码 | `cspace.rs:95` | `hash_index` 插入后**从不被查询**；`find_by_name` 用 `format!("cap:{}", i)` 拼字符串 |
| `CapGrant` 空实现 | `syscall_cap.rs:79-85` | 返回 `ok(0)`，不做任何跨任务能力转移 |
| 派生检查形同虚设 | `syscall_cap.rs:32-38` | 父槽不存在时**直接跳过检查**，然后照常 `allocate` |
| `CSpace::allocate` 不校验 parent 合法性 | `cspace.rs:77-81` | 只要 `Some(idx)` 就建派生边，**可伪造任意祖先** |
| `revoke_with_descendants` 无原子性 | `revocation.rs:126-161` | BFS 后逐个 `revoke`，中途失败即半撤销 |
| 无“拒绝路径”测试 | `caplet-cap` 12 个测试 | **没有一条验证“无权限被拒绝”或“权限不能放大”** |

**影响**：README 的核心卖点“以能力为核心安全原语”目前**不可验证、不可用**。

**建议**：`Cap<T>` 改为持有 `&'static ResourceTable` + `Handle<T>`，`lookup` 时校验 `slot.generation == handle.generation`；补“权限不能放大”“派生链撤销原子性”“旧句柄失效”三条 P0 测试。

---

### P0-11　动态加载器安全假设全部不成立（五处叠加）

**A. ELF 解析丢失段布局（可导致任意代码执行）** — `crates/caplet-loader/src/elf.rs:220-236`

```rust
// Concatenate segments into a single image.
let mut image = Vec::new();
for (ph, slice) in &segments {
    ...
    image.extend_from_slice(slice);   // <- 不管 p_vaddr / p_memsz
}
```

多个 `PT_LOAD` 段被**首尾拼接**，`p_vaddr` 的间隙、`p_memsz > p_filesz` 的 BSS 部分被完全丢弃；镜像内部**没有按 vaddr 重排**。任何含 `.text` + `.data` + `.bss` 的真实 ELF 都会被加载到错误地址，**执行随机数据**。

**B. 签名校验默认放行** — `verifier.rs:135-140` + `sandbox.rs:85-94`

```rust
#[cfg(not(feature = "crypto"))]
fn crypto_verify(...) -> Result<(), SignatureError> { Err(SignatureError::Unavailable) }
…
Err(verifier::SignatureError::Unavailable) => { /* Crypto backend missing; accept and continue. */ }
```

默认 feature 下，所有 `.capp` 模块**无条件通过签名检查**。即使开启 `crypto`：实测 `cargo test -p caplet-loader --features crypto` **有 6 个编译错误**（`E0308` ×4、`E0063 missing field is_infinity`、`E0507`），**根本没有可用的签名后端**。

**C. `load_module` 不拷贝镜像** — `sandbox.rs:104-127` 注释自承 *“We do not have a direct write to physical memory from the loader crate”*，然后返回 `LoadedModule { base_vaddr, base_paddr, … }`。**镜像从未被写入任何目标地址。**

**D. 边界检查可整数溢出** — `parse_elf` 中 `e_phoff as usize + e_phnum * size_of::<Phdr>()` 在 32 位下可溢出后绕过检查；也未校验段是否重叠、`p_memsz` 是否为 0。

**E. required_caps 从不校验** — `manifest.rs` 定义了 `required_caps` / `granted_caps`，但 `sandbox.rs::load_module(…, _loader_caps: &CSpace)` 的 loader CSpace **参数带下划线、完全不用**，所谓“required_caps 必须在 loader 的 CSpace 中存在”这一安全约束**从未被检查**。

**建议**：重写 `elf.rs` 按 `p_vaddr` 真正布局；签名“Unavailable ⇒ 拒绝”作为默认；`load_module` 必须接收目标缓冲区并真正拷贝。

---

### P0-12　安全 / OTA / TRNG 全是“假的安全”

| 位置 | 问题 |
|---|---|
| `syscall_crypto.rs:66-74` | `check_crypto_token()` **`return true`** — 任何任务都能调用加密/TRNG syscall |
| `syscall_crypto.rs:131-157` | CBC 加解密**未实现**，返回 `ERR_UNSUPPORTED` |
| `syscall_crypto.rs:266-277` | `soft_trng_fill` 是**固定种子 `0xDEADBEEF…` 的 LCG**，纯 `static mut`，中断/多核下数据竞争。**任何依赖它生成密钥的代码都不安全** |
| `syscall_crypto.rs:429-442` | AES **解密轮缺 `InvMixColumns`**（注释自承 “Simplified inv round”）→ 解密结果错误 |
| `syscall_flash.rs:112-131` | `check_flash_token()` **`true`**；`validate_region()` **`Ok(())`**；`is_writable()` **`true`** → 任何任务可读写任意 flash |
| `syscall_flash.rs:42` | `let src = (offset as usize) as *const u8;` — 把用户给的“偏移”当**绝对物理地址**，配合上一条 = **任意内存读** |
| `caplet-ota/src/verifier.rs:131-159` | `read_signature()` 恒返回 `UnknownKey`，`verify_signature()` 恒 `false` → **OTA 永远无法通过校验**（等于 OTA 不可用） |
| `caplet-ota/src/verifier.rs:114-129` | `compute_sha256` 在未开 `crypto` 时返回 **`[0u8;32]`** —— 若“预期全 0”，则**任意篡改镜像都能通过哈希校验** |
| `caplet-ota/Cargo.toml` | `crypto = ["caplet-loader/crypto"]` —— 由于 `caplet-loader` 的 crypto 本身编译不过，**这个 feature 永远打不开**，两条 `#[cfg]` 分支等价，校验逻辑是死代码 |
| `caplet-ota/src/boot_manager.rs:184-204` | `write_u32/u16` 直接 `write_volatile` 写 flash —— **没有解锁序列、没有页擦除**。真实 NOR flash 上 1→0 只能靠擦除，写入必然失败或静默损坏 |
| `caplet-ota` 整 crate | **0 个单元测试** |

**建议**：S4 阶段重做。在此之前，任何“安全”宣称都应在文档中明确标注为“未实现”。

---

### P0-13　文档与代码严重不符（README 里程碑、SUMMARY、RENOVER_SUMMARY）

1. `README.md` M1「模型完整 + 可跑」勾选中含“静态优先级调度器”，但**未接入**（P0-01）。
2. `README.md` M3 Step 1–22 **全部勾选**，但：
   - Step 5 “Cap<MemoryRegion> ↔ MMU 同步 syscall”—— `syscall_cap.rs:130` 用的是 `let mut mmu = NoMmu;`，**永远空操作**；
   - Step 7 “SMP 启动（PSCI + secondary_main + GIC SGI）”—— `ports/cortex-a` 中**无任何 PSCI/SGI 代码**（grep `psci` 无结果）；
   - Step 13 “ed25519 签名校验”—— **编译不过**（P0-11 B）；
   - Step 22 “最终验收（workspace 测试 0 失败）”—— **实测 `cargo test --workspace` 失败**。
3. `SUMMARY.md` 与 `RENOVER_SUMMARY.md` 写“QEMU Verification (All Passing)”，但：
   - `hello-qemu-cm4` 的 run 命令里 `-cpu cortex-m4` 在本机 QEMU 11.1 上**直接报 `Invalid CPU model: cortex-m4`**（mps2-an500 只接受 `cortex-m7`）；
   - “Full verification of UART output pending”（SUMMARY 原话）与标题的 “All Passing” 自相矛盾。
4. `README.md` 快速开始仍写 `--target thumbv6m`，但 `boards/hello-qemu-cm3` 实际已切到 `thumbv7m-none-eabi`。
5. 仓库根目录堆积 **50+ 个**构建产物 / 临时文件（`cm3full.log`、`ht.log`、`q.out`、`aa_out.txt`、`test.resc`、`output.map`…），全部未纳入 `.gitignore`。

**建议**：README 的勾选状态改由 CI 产物自动生成，杜绝手工打勾。

---

### P0-14　`examples/` 与 `tools/kani/` 是“孤岛工作区”，从不被构建

- `examples/Cargo.toml` 与 `tools/kani/Cargo.toml` 自带 `[workspace]`，**不属于**根 workspace 的 members，根 `Cargo.toml` 也未列 `examples/*`。
- 因此 `examples/caplet-ui-demo`、`tools/kani` **从未被编译过**。

**建议**：并入根 workspace 并加 CI 构建，或明确标注为“外部示例”并从 README 结构图中删除。


## 3. P1 —— 功能性 / 架构性缺陷（24 项）

### P1-01　`current_cpu()` 恒返回 CPU 0（SMP 名存实亡）
`crates/caplet-kernel/src/cpu.rs:97-99` 函数体就是 `CpuId(0)`。注释说“端口层预期用 TPIDRRO_EL0 覆盖”，但 grep 全仓库**没有任何端口覆盖**。`SmpScheduler::tick()` 因此**永远在 CPU 0 的队列上工作**。

### P1-02　`find_pid_for_task()` 返回“进程表第一个”，与 task_id 无关
`kernel.rs:62-73` 注释直言 “In a full implementation, we'd maintain a task -> process mapping”，实际返回 `self.processes.keys().next()`。**任何任务的 CSpace 都是同一个进程的。**

### P1-03　`with_current_cspace_into_static()` 凭空制造 `&'static mut`
`kernel.rs:76-85` 从 `&mut BTreeMap` 取 `&mut CSpace` 再转 `&'static mut`，是 **UB**（可被其他代码同时持有），Miri 必报。

### P1-04　`init_bump_allocator(ram_start, ram_size)` 让 bump 分配器覆盖 `.bss` 起点的整段 RAM
`kernel.rs:155-158` 使用 `MemoryLayout::qemu()`（`ram_start=0x2000_0000, ram_size=64KB`），而 `hello-qemu-cm4/memory.x` 里 `.bss` 恰好从 `0x2000_0000` 起。同时板子的 `#[global_allocator]` 用的是 `0x2010_0000..0x2020_0000` —— **两套堆互不知情**。`syscall_memory::AllocateMemory` 走 bump 分配器 → 用户堆会覆盖 `.data`/`.bss`。

### P1-05　`#[task]` 的栈大小 / 优先级 / 名字全部被丢弃
- `caplet-macros/src/task_macro.rs:80-82`：生成的包装 fn 只是 `fn #ident() -> impl Future`，`priority` 与 `stack` 只写进 `TaskDescriptor` 静态量，**没有任何代码读取它**。
- `caplet-task/src/task_context.rs:26-34`：`TaskContext::new(entry, sp, arg)` **完全不用 `sp` 和 `arg`**（编译告警 “unused variable: `arg`”）。
- `ports/cortex-m/src/context.rs:60-86`：同样 `sp` 未使用，`stack_pointer()` 恒返回 0。

### P1-06　`Executor::run_once` 持锁 poll
`caplet-async/src/executor.rs:171-218`：`let mut guard = self.inner.lock();` 之后在**持锁状态**下 `task.future.as_mut().poll(&mut cx)`。若 future 内部 `spawn`（需要同一把锁）则**自死锁**。

### P1-07　`Mutex::lock()` 从不阻塞，永远自旋
`caplet-sync/src/mutex.rs:75-87`：`loop { try; spin_loop(); }`。单核上持锁任务若被抢占，等待者**永远等不到**（被抢占的正是持锁者），直接活锁。文档自称“priority-aware mutex”，**完全没有优先级继承**。

### P1-08　`Semaphore::acquire()` 返回 `bool` 且永不失败，无限自旋
`caplet-sync/src/semaphore.rs:70-81`：永远返回 `true`，调用方无法区分“成功获取”与“永远阻塞”。无超时、无唤醒机制。

### P1-09　`Event::waiters` 计数器只增不减
`caplet-sync/src/event.rs:86,102,127`：`inner.waiters += 1` 在自旋循环里每轮都加一次 → 计数器爆表，`waiter_count()` 返回无意义值。

### P1-10　`ReadyQueue` 用 `BinaryHeap` + 全量重建，与文档的 O(1) 不符
`ready_queue.rs`：`remove()` 是 O(n) 重建；`dequeue()` 后 `rebuild_bitmap()` 又是 O(n)。README 宣称“静态优先级调度器，O(1)”。应改成 32 个 `VecDeque` + 位图。

### P1-11　优先级位图是死代码
`ready_queue.rs:27` 维护 `priority_bitmap`，但 `highest_priority()` 在全仓库**无引用**；`BinaryHeap` 的堆序与位图两套机制并存，其中一套完全无效。

### P1-12　`Scheduler::schedule()` 的“重新入队 current”依赖调用顺序，无状态机保证
`scheduler.rs:224-253`：只要 `current` 存在就无条件 `enqueue`。若调用方忘记先 `block_current()` / `terminate()`，同一任务会同时出现在 `tasks` 表与队列中，且 `task_count()` 与实际可运行数不符。

### P1-13　`MapMemory` 有两份互相冲突的空实现
- `syscall_dispatch.rs:36-41` 把 `MapMemory/UnmapMemory` 交给 `handle_memory_syscall`（`syscall_memory.rs`），注释写 *“TODO: integrate with MPU/MMU；For now, just acknowledge the mapping”*，返回一个**虚构的 region id**。
- `syscall_cap.rs:86-99` 另有一份同名实现，用 `NoMmu`（永远 `Ok`）。
- **两份实现、两种语义、都不可用。**

### P1-14　`FlashRead` 把“偏移”当物理地址；`FlashWrite/Erase` 写进 RAM 镜像
`syscall_flash.rs:42,136-148`：`offset as usize as *const u8`；`platform_flash_write` 调 `simulate_flash_write`（写一个 RAM 数组）。**在任何真实板子上都是错觉。**

### P1-15　`TimerSet / TimerCancel` 空实现；`TimerWheel` 从未被内核引用
`syscall_timer.rs:68-92` 注释 *“For now, just acknowledge”*。`caplet-timer` 的 `TimerWheel` 在全仓库 grep 中**只出现在自身测试里**。

### P1-16　`TaskSleep` 就是 `yield`
`syscall_task.rs:80-87` 注释 *“we treat sleep as yield — there is no timer wheel yet (Phase 5)”*。**`sleep(10s)` 实际是“让出一次 CPU 后立即返回”。**

### P1-17　`CURRENT_TASK_ID` 全仓库**零次赋值**（最关键的一条 P1）
`crates/caplet-kernel/src/kernel.rs:24` 只有 `AtomicUsize::new(0)` 的声明，**没有任何 `store`**。因此 `current_task_id()` 恒为 0 → `TaskExit` / `TaskYield` / `TaskGetId` / IPC 全部作用于“任务 0”。

### P1-18　IPC 状态机不完整 + 全局单例泄漏
- `syscall_ipc.rs:81` `static CURRENT_REPLY: SpinLock<Option<ReplyContext>>` —— **全局单例**，任意时刻只能有一个任务处于“等待回复”状态。
- `syscall_m3.rs:41-48` `ns_create` **硬编码名字 `"user_ns"`**，忽略调用方参数。
- `ns_publish` / `ns_subscribe` / `ns_endpoint` / `module_load` / `module_start` **全部空实现**（`ok(0)` / `err(1)`）。

### P1-19　`caplet-fs` 的 syscall 用“路径字符串末 8 字节当句柄”
`fs/syscall.rs:146-160`：`path_to_handle(path)` 取 `path.as_bytes()[len-8..len-4]` 解析成 u32。`close`/`read`/`write` 的“路径”必须包含二进制垃圾 —— **不可用也不可审计**。`Rename` 用 `'\0'` 分隔（`:129-138`），但 `Path::parse` 在前面已处理过，`sub` 中不可能有 `\0`。

### P1-20　LittleFS / FAT32 是纯占位符
`fs/littlefs.rs` 与 `fs/fat32.rs` 各约 100 行，**每个方法都返回 `Err(DriverError::Unsupported)`**。README 声称 M2 完成 “LittleFS / FAT32”。

### P1-21　网络栈 `Send` 是空实现；TCP 只有首部解析
- `net/syscall.rs:96`：`NetSyscall::Send => Ok(NetResult::Count(buf.len()))` —— **不发送任何字节**。
- `net/tcp.rs` 全文 137 行，只有 `parse_tcp()`，**没有状态机、没有重传、没有窗口**。
- `net/dhcp.rs`：`build_discover()` 生成的报文**缺 BOOTP 的 236 字节固定字段与 xid**，`apply_offer()` 忽略 `_offered` 参数 → **DHCP 永远拿不到地址**。
- 头注释已标注 “解析 only / Stub”，但 README / ADR-0006 / ADR-0009 声称“M2 完成自研 TCP/IP”。

### P1-22　`caplet-log` 的 writer 注册方案会静默丢 writer
`log/lib.rs:74-89` 用“对象地址低 7 位”当 tag，**必然碰撞**；`current_writer()` 只认 semihosting，其它 writer 一律返回 `None` → **UART writer 装了等于没装**。`--all-features` 时 `uart.rs:39` 的 `impl<U: Uart> LogWriter for UartWriter<U>` 与 `LogWriter: Sync` 冲突（`U` 未要求 `Sync`），**编译失败**。

### P1-23　`caplet-timer` 在裸机上无法编译；时间窗仅 128 tick
- `crates/caplet-timer/src/wheel.rs:20`：`use core::sync::atomic::{AtomicU64, Ordering};` —— **`thumbv7em` 无 `AtomicU64`**（实测 `E0432`）。作者在 `kernel/tickless.rs` 已写了“双 `AtomicU32` 模拟”的范式，timer crate 没照做。
- 散列函数 `bucket = (deadline / 8) % 16` → **仅覆盖 128 个 tick**，任何超过 128 tick 的定时器会被**错误地提前触发**。容量也只有 `16*8*16 = 2048`。

### P1-24　三个 demo 板与 `caplet-hal-nrf52` 的 API 漂移
- `industrial-demo` 调用的 `MockSensor::read()`、`Reading.device_id`、`f32::fract` 在实际 crate 中**都不存在**（`E0599` / `E0609`）。
- `medical-device` / `automotive` / `energy-meter` 板内自建 `#[panic_handler]`，与 `caplet-cortex-m`（默认 `panic-handler` feature）冲突 → `E0152 duplicate lang item panic_impl`。
- `caplet-hal-nrf52/src/timer.rs:26` 实现的是 `TimerTrait`，而 `caplet-hal` 导出的名字是 `Timer` —— 【推断】这一处要么是内部重命名，要么是 trait 名漂移，nrf52 的 0 个测试无法发现。

---

## 4. P2 —— 质量 / 一致性（17 项）

| 编号 | 问题 | 位置 |
|---|---|---|
| P2-01 | `Priority::HIGHEST` 与 `Priority::IDLE` 的用法没有强制区分，任何任务都可以登记为空闲优先级 | `priority.rs` |
| P2-02 | `CpuId::new()` 饱和到 3，`CpuId(99)` 直接构造则越界；`index()` 无检查 | `cpu.rs` |
| P2-03 | `balance_load()` 末尾 `self.pinned_to.lock().insert(victim, l)` **把 task 永久钉死在 `l`**，与相邻注释 “deliberately no affinity rewrite” 直接矛盾 | `smp_scheduler.rs:482` |
| P2-04 | `ReadyQueue::enqueue` 不查重；`Scheduler::make_ready` 可重复入队同一任务 | `ready_queue.rs` / `scheduler.rs:187` |
| P2-05 | `cspace.rs:187` `active_count -= 1` 无下溢保护 | `cspace.rs` |
| P2-06 | `generate_hash_key` 的 hi 只在 `lo == 0` 时递增，而 `lo` 从 1 开始永不为 0 ⇒ **hi 永远是 0** | `cspace.rs:262-271` |
| P2-07 | `cap_revoke` 三种 mode 全部 `revoke_fast`，Slow 分支注释 “no-op stub” | `revocation.rs:165-181` |
| P2-08 | `impl Default for RamFs` 调 `RamFs::new()` 后 `unreachable!()` → 任何 `RamFs::default()` **必 panic** | `ramfs.rs:139-146` |
| P2-09 | `CreateTruncate` 不截断已有文件 | `ramfs.rs:189-191` |
| P2-10 | `bind()` 的端口冲突检查跨**所有** socket 类型（TCP 与 UDP 同端口也冲突）；`push_rx` 用 `next_rx_seq + rx.len()` 作 key，乱序/重传会产生空洞 | `socket.rs:107,140` |
| P2-11 | 【推断】`LoopbackInterface.ipv4: Cell<Ipv4Addr>` 使 `!Sync`，但 `NetworkInterface` 被当作线程安全使用 | `interface.rs:71` |
| P2-12 | 错误码硬编码为魔法数字且跨文件冲突（`ERR_NO_TOKEN = 1` 同时出现在 crypto 与 flash） | `syscall_*.rs` |
| P2-13 | `Syscall::from_number` 用 `match num { 1 => … }` 手写映射，与 `#[repr(u32)]` 枚举值重复维护 | `syscall.rs:91-131` |
| P2-14 | `kernel/boot.rs:106` 用 `#[cfg(feature = "cortex_m_idle")]`，但该 feature **在 `caplet-kernel/Cargo.toml` 中不存在**（只有 `soft-crypto`） | `kernel/boot.rs` |
| P2-15 | `cap_init()` 与 `cap_init_with_config()` 各写一个 `static mut MaybeUninit<Kernel>`，**第二次调用 UB**，且与 `init_kernel()`（`Box::leak` + `KERNEL_PTR`）是**两套独立的内核实例** | `kernel_init.rs:15-38` |
| P2-16 | `disable_interrupts()` / `init_memory()` / `kernel_init()` 全是**空函数体 + 注释** | `ports/cortex-m/src/boot.rs` |
| P2-17 | 编译告警总量：kernel 50、riscv 40、cortex-a 17、caplet-cap 8、security 5、loader 4、cortex-m 3、ota 3 | `cargo build` |

---

## 5. 构建 / 工具链 / 仓库卫生

| 项 | 现象 | 建议 |
|---|---|---|
| `.cargo/config` 与 `.cargo/config.toml` **同时存在** | cargo 每次构建都警告 `both … exist. Using .cargo/config`；真正生效的 `config`（无扩展名）**只有一行 linker 设置**，`config.toml` 里的所有 runner 全部失效 | 合并为一个文件 |
| 13 个 crate 自带 `[profile.release]` | `warning: profiles for the non root package will be ignored` | 全部删除，统一到根 `Cargo.toml` |
| `Cargo.toml` 中 `caplet-timer = { workspace = true }` 写在 `[workspace.dependencies]` 内 | `warning: unused manifest key`（自引用），非法 | 删除该行 |
| `hashbrown` 默认 features | 启用 `ahash`；【推断】裸机环境下哈希种子来源不可控，若用于安全相关容器存在碰撞攻击面 | 显式 `default-features = false, features = ["alloc"]` |
| 仓库根目录 50+ 日志 / 临时文件 | `cm3full.log`、`ht.log`、`q*.out`、`aa_out.txt`、`test.resc`、`output.map`、`build.log` … | 加 `.gitignore`，清理 |
| `deny.toml` 存在但 `cargo deny` 在 CI 中 `continue-on-error` | 许可证 / 漏洞门禁失效 | 去掉 `continue-on-error` |
| 无 `Miri` / `cargo-unsafe` 检查 | 大量 `unsafe`（伪造 `&'static mut`、`static mut`、裸指针）从未被动态检查 | CI 加 `cargo +nightly miri test -p caplet-cap -p caplet-sync` |
| 未安装 `kani` | `tools/kani/*.rs` 从未运行 | CI 中把 kani 失败当作失败 |
| `tests/qemu-cm3/run.sh:19`、`tests/qemu-cm4/run.sh:19` | 硬编码 `"/c/Program Files/qemu/qemu-system-arm.exe"`，非 Windows 环境直接失败；`qemu-cm4/run.sh:21` 还有转义错误 | 改用 `${QEMU_SYSTEM_ARM:-qemu-system-arm}` |
| `tests/renode-*/run.sh` | 只覆盖 allwinner 一块板；SUMMARY 声称有 4 块 | 恢复 4 块板的 Renode 覆盖并在 CI 中跑 |
| `ports/riscv` 40 个告警；板子编译不过 | 端口完全无从测试 | 先修 hello-riscv |

---


## 6. 下一阶段路线图（可执行、有验收标准）

> **原则**：先让“真实的东西”变真，再让“假的东西”露馅。每一阶段结束必须有**一条可自动判定的验收标准**。

### 阶段 S0 —— 止血：让构建与 CI 说真话（预计 3～5 人日）

| # | 任务 | 验收标准 |
|---|---|---|
| S0-1 | 修 `caplet-hal-stm32f4` 的 47 个 `missing_docs`（`raw.rs` 用 `#[allow(missing_docs)]`，公共 API 补文档） | `cargo build -p caplet-hal-stm32f4` 通过；`nucleo-f446ze`、`hello-nucleo-f446ze-qemu` 干净构建通过 |
| S0-2 | 修 `caplet-log` `--all-features`（`UartWriter` 的 `impl` 加 `U: Sync` bound，修 `&mut` bug） | `cargo build -p caplet-log --all-features` 通过 |
| S0-3 | 修 `caplet-loader --features crypto` 的 6 个编译错误，或**删除该 feature 的代码并声明“未实现”** | `cargo test -p caplet-loader --features crypto` 通过 |
| S0-4 | 去掉 `managed 0.8` 依赖（换 `heapless` 的 `FnvIndexMap` 或自写） | `hello-qemu-cm4` 干净构建通过 |
| S0-5 | 修 `caplet-timer` 的 `AtomicU64`（改双 `AtomicU32`，参考 `kernel/tickless.rs`） | `hello-qemu-cm4` 干净构建通过 |
| S0-6 | 修 `hello-riscv`（`#![no_std]` / `#![no_main]` / `#[global_allocator]` / `#[panic_handler]` / 移除 host `main`） | `cargo build -p hello-riscv --features target-none --target riscv32imac-unknown-none-elf` 通过 |
| S0-7 | 修 3 个 demo 的 `E0152`（板内自带 `#[panic_handler]` 与 `caplet-cortex-m` 冲突） | `medical-device` / `automotive` / `energy-meter` 干净构建通过 |
| S0-8 | 修 `industrial-demo` 的 3 个 API 不匹配（`MockSensor::read`、`Reading.device_id`、`f32::fract`） | 干净构建通过 |
| S0-9 | 删除 `.cargo/config`（或合并）；删除 13 个 crate 内 `[profile.*]`；删除 `workspace.dependencies.caplet-timer` 自引用 | `cargo build` 无 profile / manifest 警告 |
| S0-10 | CI 修正：`rust-toolchain`；clippy 不带 `--all-features`；加 `cargo test --workspace`；QEMU 测试改为 grep 断言并据其返回退出码 | CI 绿 |
| S0-11 | 清理仓库根目录 50+ 临时文件 + 更新 `.gitignore` | `git status` 干净 |
| S0-12 | **更新 README / SUMMARY，把未实现的勾去掉**，并加一行“当前状态：演示级” | 文档与 CI 一致 |

**S0 出口标准**：`cargo test --workspace` 通过；所有板子**在干净目录**构建通过；CI 绿；文档不撒谎。

---

### 阶段 S1 —— 让内核真正跑起来（预计 10～15 人日）

| # | 任务 | 验收标准 |
|---|---|---|
| S1-1 | 重写 `ports/cortex-m/src/context.rs` 的 `__caplet_switch`（改用 `global_asm!` + 固定寄存器） | QEMU 集成测试：两个任务各自累加共享计数器，5 秒后**两者都 > 0** |
| S1-2 | 修 `pend_pendsv` 的 `str r1, [r0, #4]` → `[r0]`，去掉 `cpsid/cpsie` | 同上 |
| S1-3 | 接入 `SVCall` 异常 → `syscall_dispatch`，并在每次上下文切换时 `store` `CURRENT_TASK_ID` | 板级测试：用户线程发 SVC，内核正确返回；`TaskGetId` 返回真实 id |
| S1-4 | 让 `#[task]` 宏的 `priority` / `stack` 真正注册进 `Kernel`（编译期产出 `TaskDescriptor` 静态表，启动时枚举） | QEMU 上 3 个不同优先级任务，高优先级先跑且能抢占低优先级 |
| S1-5 | 板子 `main()` 改为调用 `kernel_start()`，`Executor` 降级为“内核里的一个任务” | `hello-qemu-cm4` 的 `network`（priority 4）**必须**在 `control`（priority 3）让出后打印 |
| S1-6 | 重写 `SmpScheduler::tick()`，去掉“复制任务”逻辑；任务表独立于队列 | 单元测试：3 任务、10 次 tick，队列 + current 总数恒为 3 |
| S1-7 | `current_cpu()` 改为端口注入（`Arch::current_cpu()`） | Cortex-A 下两核返回不同 id |
| S1-8 | `ReadyQueue` 改为“32 个 `VecDeque` + 位图”，文档与实现对齐 | 基准：`enqueue` / `dequeue` 与任务数无关 |
| S1-9 | `TimerWheel` 接入内核，`TaskSleep` 真正休眠；tickless 走 `enter_idle(next_deadline)` | QEMU：`sleep(1000ms)` 后唤醒，前后 tick 差 ≈ 1000 |
| S1-10 | `Mutex` / `Semaphore` / `Event` 改为**阻塞式**（挂起当前任务到等待队列） | 单元测试：持锁任务睡眠时，等待者不被饿死 |

**S1 出口标准**：QEMU 上“优先级抢占 + 时间片轮转 + 定时器唤醒 + 阻塞同步原语”全部可观测；`caplet-kernel` 的 `Scheduler` / `SmpScheduler` 不再是死代码。

---

### 阶段 S2 —— 让能力系统真的能授权（预计 10～12 人日）

| # | 任务 | 验收标准 |
|---|---|---|
| S2-1 | `Cap<T>::get()` 接入 `ResourceTable`（`kernel.rs` 已有 `resources` 字段但**从未被使用**） | 单元测试：持 `Cap<Uart>` 能写；无 cap 返回 `CapError::NotFound` |
| S2-2 | 句柄 generation 校验（槽位复用后旧句柄必须失败） | 单元测试：revoke → 复用 → 旧句柄返回 `Err(Revoked)` |
| S2-3 | `CSpace::allocate` 校验 parent 存在且被授予 `DERIVE` | 单元测试：伪造 parent → `Err(NotFound)` |
| S2-4 | 实现 `CapGrant` 跨任务转移 | 单元测试：任务 A 把 cap 转给 B，B 能用、A 的句柄失效 |
| S2-5 | `revoke_with_descendants` 原子化（先标脏、后统一提交） | 单元测试：中途失败时 `active_count` 不变 |
| S2-6 | `syscall_flash` 接上真实 `FlashToken` + 区域校验；`syscall_crypto` 接上 `CryptoToken` | 单元测试：无 token 返回 `ERR_NO_TOKEN`；越界 offset 返回 `ERR_OUT_OF_REGION` |
| S2-7 | `soft_trng_fill` 改为接入硬件 TRNG；无硬件时**返回 `Unsupported` 而不是伪随机** | 单元测试：TRNG 两次输出不同；无后端时报错 |
| S2-8 | AES 解密补 `InvMixColumns`，加 NIST FIPS-197 测试向量 | 单元测试：AES-128 已知答案测试通过 |
| S2-9 | `find_pid_for_task` 改为真实映射（`CURRENT_TASK_ID → pid`） | 三进程测试：各自 cap 互相不可见 |

**S2 出口标准**：`caplet-cap` 新增 ≥ 20 条“拒绝路径”测试；Kani 的 `cap_derive` harness 能真正证明“不得放大权限”。

---

### 阶段 S3 —— 让加载器 / MMU / 隔离可用（预计 12～15 人日）

| # | 任务 | 验收标准 |
|---|---|---|
| S3-1 | 重写 `elf.rs`：按 `p_vaddr` 布局、处理 `p_memsz > p_filesz`（BSS）、校验全部偏移 | 单元测试：一个真实 `cargo build` 产物能被正确解析 |
| S3-2 | `load_module` 真正拷贝镜像到目标区 | 单元测试：加载后目标内存与文件内容一致 |
| S3-3 | 签名“不可用即拒绝”，并把 `crypto` 后端换成经过审计的 Ed25519 实现 | 单元测试：篡改一个字节 → `BadSignature` |
| S3-4 | 实现 `MmuOps for ports/cortex-a::Mmu` 并注入内核 | QEMU A20：用户态访问未映射地址触发数据中止 |
| S3-5 | Cortex-M 接 MPU（`cortex-m` crate 的 `mpu`），至少保护内核 `.text` | QEMU：用户任务写内核地址触发 MemManage |
| S3-6 | 重写 `hello-allwinner-a20-qemu`，链接 `caplet-cortex-a` + 内核，输出 `caplet_kernel_entry` 日志 | QEMU：A20 板两个任务交替执行 |
| S3-7 | `ports/cortex-a` 的 `context_switch` / `start_first_task` 用 `global_asm!` 实现 | 同上 |
| S3-8 | `SmpScheduler` 接 GIC SGI + PSCI，`hello-allwinner-a20-qemu` 开第二核 | QEMU：两核分别打印自己的 `CpuId` |

**S3 出口标准**：Cortex-M 与 Cortex-A 都能“内核真实调度 + 权限真实隔离”。

---

### 阶段 S4 —— 让存储 / 网络 / OTA 真正工作（预计 15～20 人日）

| # | 任务 | 验收标准 |
|---|---|---|
| S4-1 | `caplet-fs` 接入 `littlefs2`（真实驱动，非占位符） | QEMU + 仿真 flash：写 → 断电重启 → 读回一致 |
| S4-2 | 重写 `fs/syscall.rs` 的句柄协议（句柄独立传递） | ABI 文档 + 单元测试 |
| S4-3 | `caplet-net` 实现 `Send`（真正调用 `iface.transmit`） | 环回接口：send 后 `drain_tx()` 收到完整以太网帧 |
| S4-4 | 实现 TCP 状态机（至少 CLOSED/LISTEN/SYN_RCVD/ESTABLISHED/FIN_WAIT），或**接入 `smoltcp` 并在文档中声明** | QEMU `-netdev user`：两个板子互 ping |
| S4-5 | DHCP 客户端补 BOOTP 固定字段 + xid + 真解析 | QEMU user-net：拿到 `10.0.2.15/24` |
| S4-6 | `caplet-ota` 补 ≥ 15 个测试；`BootManager` 走真实 flash 驱动（解锁-擦除-写入-校验） | 集成测试：A/B 切换 + 断电回滚 |
| S4-7 | `Verifier` 接上 S3-3 的 Ed25519 | 篡改镜像 → 拒绝启动 |

---

### 阶段 S5 —— 形式化 / 性能 / 文档（预计 10 人日，可与 S4 并行）

| # | 任务 | 验收标准 |
|---|---|---|
| S5-1 | CI 中真实运行 Kani（去掉 `|| true`），补齐 `scheduler` / `cspace` / `ready_queue` harness | Kani 全绿 |
| S5-2 | Kani 覆盖“能力不放大”“调度器不丢任务”“队列不重复”三条不变量 | 同上 |
| S5-3 | `cargo +nightly miri test` 覆盖 `caplet-cap` / `caplet-sync` / `caplet-async` | Miri 无 UB |
| S5-4 | 消除全部编译告警（`-D warnings`） | CI |
| S5-5 | 实测上下文切换 / 中断延迟并写入文档（目前**无任何实测数据**） | `docs/benchmarks.md` |
| S5-6 | README 状态改为“由 CI 生成”；删除 `SUMMARY.md` / `RENOVER_SUMMARY.md` 中的夸大表述 | 文档 |

---

## 7. 完成度雷达（现状 → 目标）

| 能力 | 现状 | 目标（M2 出口） | 对应阶段 |
|---|---:|---:|---|
| 构建与 CI 可信 | 20% | 100% | **S0** |
| 抢占式调度 / 上下文切换 | 0% | 100% | **S1** |
| 系统调用边界（SVC） | 0% | 100% | **S1** |
| 时间管理（sleep / tickless / timer wheel） | 10% | 90% | **S1** |
| 阻塞同步原语 | 10% | 90% | **S1** |
| 能力授权（真实访问控制） | 15% | 90% | **S2** |
| 进程隔离 / MMU / MPU | 5% | 70% | **S3** |
| 动态加载 | 15% | 80% | **S3** |
| 持久化文件系统 | 25% | 85% | **S4** |
| 网络（TCP / UDP / DHCP） | 15% | 75% | **S4** |
| OTA / 安全启动 | 3% | 80% | **S4** |
| HAL（STM32F4 / nRF52 / GD32） | 20% | 85% | **S0 + S4** |
| 形式化验证 | 5% | 60% | **S5** |
| **综合** | **约 26%** | **约 85%** | |

---

## 8. 附：本次审计使用的主要命令（可复现）

```bash
# 全量测试（会失败，暴露 P0-04）
cargo test --workspace

# 剔除 stm32f4 后的测试（会全绿，但不代表内核可用）
cargo test --workspace \
  --exclude caplet-hal-stm32f4 --exclude caplet-hal-gd32 \
  --exclude nucleo-f446ze --exclude hello-nucleo-f446ze-qemu

# 探测 all-features（暴露 P1-22 / P0-11B）
cargo build -p caplet-log --all-features
cargo test  -p caplet-loader --features crypto

# 干净目录构建各个板（暴露 P0-04 / P0-05 / P1-23 / P1-24）
CARGO_TARGET_DIR=/tmp/verify cargo build --release \
  -p hello-qemu-cm4 --features target-none --target thumbv7em-none-eabihf
# 其余板同理：hello-qemu-cm3(thumbv7m) / cm4-fs / industrial-demo /
# medical-device / automotive / energy-meter / nucleo-f446ze /
# hello-nucleo-f446ze-qemu / hello-riscv(riscv32imac) /
# hello-allwinner-a20-qemu(armv7a-none-eabihf)

# 实跑 QEMU（-cpu 必须是 cortex-m7，mps2-an500 不支持 cortex-m4）
qemu-system-arm -machine mps2-an500 -cpu cortex-m7 -nographic \
  -semihosting-config enable=on,target=native \
  -kernel target/thumbv7em-none-eabihf/release/hello-qemu-cm4

qemu-system-arm -machine vexpress-a9 -monitor none \
  -semihosting-config enable=on,target=native \
  -kernel target/armv7a-none-eabihf/release/hello-allwinner-a20-qemu

# 检查上下文切换符号是否真的被链接进来
llvm-nm target/thumbv7em-none-eabihf/release/hello-qemu-cm4 \
  | grep -E '__caplet_switch|__caplet_start_first'   # -> 无输出
```

---

## 9. 结语

**“现在所有的问题都解决了吗？”——没有。**

当前代码的**乐观面**是真实的：`hello-qemu-cm3`、`hello-qemu-cm4`、`hello-qemu-cm4-fs`、`hello-allwinner-a20-qemu` 都能在 QEMU 上跑出预期输出；约 270 个 host 测试通过；crate 划分清晰、文档密度高、代码风格统一。这为后续工作提供了不错的地基。

但**必须清醒地认识**：

1. 那些“通过”的测试，测的是**数据结构和纯函数**，几乎没有跨越 SVC 边界、没有跨越上下文切换、没有跨越 MMU —— 它们证明的是“补间代码正确”，而不是“内核可用”。
2. `#[task(priority=…, stack=…)]`、`Cap<T>`、`cargo test --workspace`、CI 门禁、Kani、`--all-features` —— 这些“看起来已经完成”的东西，**没有一项是真的能用**。
3. **最危险的是第 2 点**：一个能打印 `hello world` 的系统，很容易被误认为“已经跑通了 RTOS”，从而在错误的认知下继续叠加功能。**建议把第 6 节的 S0 与 S1 作为最高优先级，且在 S0 完成前不要新增任何功能。**

**预估**：S0 约 1 周、S1 约 3 周、S2 约 2.5 周、S3 约 3 周、S4 约 4 周、S5 与 S4 并行约 2 周 —— **单人全职约 13～15 周**可把“综合完成度”从 26% 推到约 85%，达成 README 中 M1 + M2 的真实交付。
