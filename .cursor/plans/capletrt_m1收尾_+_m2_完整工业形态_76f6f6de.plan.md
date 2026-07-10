---
name: CapletRT M1收尾 + M2 完整工业形态
overview: 补齐M1阻塞性硬伤（真实waker / scheduler / syscall / PendSV / Cap资源桥接 / 宏），完整闭环M2全特性（Kani/STM32F4/nRF52/Nucleo-F446ZE/资源预算/Tickless/LittleFS/FAT32/LWIP+MQTT-SN/工业demo）
todos:
  - id: P0.1
    content: Phase 0.1 修复 caplet-async 真实 waker
    status: completed
  - id: P0.2
    content: Phase 0.2 修复 scheduler 与 PendSV 真实上下文切换
    status: completed
  - id: P0.3
    content: Phase 0.3 真实 syscall handler（cap/task/ipc）
    status: completed
  - id: P0.4
    content: Phase 0.4 Cap 资源真实桥接 + slow path 撤销
    status: completed
  - id: P0.5
    content: Phase 0.5 宏真正生成代码（task/main）
    status: completed
  - id: P0.6
    content: Phase 0.6 cortex-m-rt 真实启动 + boards demo 走内核
    status: completed
  - id: P0.7
    content: Phase 0.7 全局状态 Atomic 化 + log 真实 backend
    status: completed
  - id: P0.8
    content: Phase 0.8 Tickless 桩与 WFI 接入
    status: completed
  - id: P1.1
    content: Phase 1.1 Kani 接入 + tools/kani scaffold
    status: completed
  - id: P1.2
    content: Phase 1.2 全部 25 syscall Kani harness
    status: completed
  - id: P1.3
    content: Phase 1.3 derivation / priority_ceiling / audit harness
    status: completed
  - id: P2.1
    content: Phase 2.1 HAL trait 完善（uart/gpio/i2c/spi/timer/interrupt）
    status: completed
  - id: P2.2
    content: Phase 2.2 HAL SemVer 与版本门
    status: completed
  - id: P2.3
    content: Phase 2.3 STM32F4 HAL 完整实现
    status: completed
  - id: P2.4
    content: Phase 2.4 nRF52 HAL 完整实现
    status: completed
  - id: P2.5
    content: Phase 2.5 安全关键外设包装（crypto/flash/jtag）
    status: completed
  - id: P3.1
    content: Phase 3.1 Nucleo-F446ZE board crate
    status: completed
  - id: P3.2
    content: Phase 3.2 CI build-f446ze + qemu-stm32f4 仿真
    status: completed
  - id: P3.3
    content: Phase 3.3 GD32 HAL 占位
    status: completed
  - id: P4.1
    content: Phase 4.1 任务配额（memory/cpu/message）数据结构与检查
    status: completed
  - id: P4.2
    content: Phase 4.2 预算拒绝事件 + Event 通道
    status: completed
  - id: P5.1
    content: Phase 5.1 Timer wheel 算法 + SysTick variable reload
    status: completed
  - id: P5.2
    content: Phase 5.2 idle_task 真实 WFI 通路
    status: completed
  - id: P5.3
    content: Phase 5.3 Tickless 验证脚本与 QEMU 断言
    status: completed
  - id: P6.1
    content: Phase 6.1 VFS 抽象与 syscall_fs
    status: completed
  - id: P6.2
    content: Phase 6.2 LittleFS 后端 + powercut 测试
    status: completed
  - id: P6.3
    content: Phase 6.3 FAT32 后端 + 预制 image 集成
    status: completed
  - id: P6.4
    content: Phase 6.4 RAMFS 调试后端
    status: completed
  - id: P6.5
    content: Phase 6.5 boot 时挂载 + qemu-cm4-fs 验证
    status: completed
  - id: P7.1
    content: Phase 7.1 NetworkInterface + Cap<Socket> + syscall_net
    status: completed
  - id: P7.2
    content: Phase 7.2 自研 LWIP 风格 eth/ip/icmp/udp/tcp/dhcp/dns
    status: completed
  - id: P7.3
    content: Phase 7.3 PHY HAL（STM32 + DP83848）
    status: completed
  - id: P7.4
    content: Phase 7.4 网络 Kani 覆盖
    status: completed
  - id: P7.5
    content: Phase 7.5 socket 能力派生与 Drop 释放
    status: completed
  - id: P8.1
    content: Phase 8.1 MQTT-SN v1.2 实现
    status: completed
  - id: P8.2
    content: Phase 8.2 Sensor trait + TMP102 实现 + mock
    status: completed
  - id: P8.3
    content: Phase 8.3 industrial-demo board（sample + upload 双 task）
    status: completed
  - id: P8.4
    content: Phase 8.4 端到端测试 + rumqttd broker 验证
    status: completed
  - id: P8.5
    content: Phase 8.5 CI 集成 industrial-demo job
    status: completed
  - id: P9.1
    content: Phase 9.1 每个 crate README 交付（11 份）
    status: completed
  - id: P9.2
    content: Phase 9.2 7 份 ADR
    status: completed
  - id: P9.3
    content: Phase 9.3 安全模型 / porting / kani / industrial 文档
    status: completed
  - id: P9.4
    content: Phase 9.4 CI 全绿门禁（lint/grep/deny/bench/kani/job）
    status: completed
  - id: P10
    content: Phase 10 fallback 机制与性能/安全门
    status: completed
isProject: false
---

# CapletRT M1 收尾 + M2 完整工业形态（2027 Q1）

> 本计划对应 `docv1.md §12` 的 M1 硬伤补齐 + M2 全部 8 项交付。
> 输出物：每个 crate README + 工业 demo + CI 全绿 + Kani 全 25 syscall 通过。

---

## Phase 0 — M1 硬伤收尾（必须先做，否则 M2 无验证基础）

> 全部阻塞项必须在 Phase 1 启动前关闭。每个 todo 都附带可验证产物。

### 0.1 修复 async 执行器真实驱动（`crates/caplet-async/`）

- [ ] **0.1.1** 把 [`crates/caplet-async/src/waker.rs`](crates/caplet-async/src/waker.rs) `into_waker()` 由 `Waker::noop()` 改为构造指向 `WakeQueue` 的真实 vtable（含 `wake / wake_by_ref / clone / drop`），raw pointer 指向 `WakeQueue` 与 `TaskId`。
- [ ] **0.1.2** 在 [`crates/caplet-async/src/wake_queue.rs`](crates/caplet-async/src/wake_queue.rs) 增加 `take_ready_snapshot()`，返回当前就绪任务 list（move 出 queue 防止 pending 时丢失）。
- [ ] **0.1.3** 重写 [`crates/caplet-async/src/executor.rs`](crates/caplet-async/src/executor.rs) `run_once()`：poll 时传入真实 waker；poll 返回 `Ready` 时把 task 标记 Complete；返回 `Pending` 时保留 waker 注册。增加 `run_until_idle()` 帮助测试。
- [ ] **0.1.4** 改 [`crates/caplet-async/src/spawner.rs`](crates/caplet-async/src/spawner.rs)：用 `Arc<Executor>` 替代 `*const Executor`，删除 unsafe impl Send/Sync；`spawn()` 不立即 drop 而是注册到 executor。
- [ ] **0.1.5** 单测：写 `tests/async_smoke.rs` —— spawn 三个 future（一个立刻 ready、一个 await 一次 timer、一个 await 两次 channel），run_until_idle 后断言执行顺序与次数。

### 0.2 修复 scheduler 与上下文切换（`crates/caplet-kernel/src/scheduler.rs`、`ports/cortex-m/src/context.rs`）

- [ ] **0.2.1** 修复 `Scheduler::schedule()` 逻辑 bug：当前 task 状态仍 `Ready` 才放回 ready_queue；否则仅切换到 next。
- [ ] **0.2.2** 修正 `crates/caplet-kernel/src/scheduler.rs::tick()`：增加时间片预算检查字段 `time_slice_ticks`（来自 [`TaskConfig`](crates/caplet-task/src/task_config.rs)），归零时触发 re-schedule。
- [ ] **0.2.3** 重写 [`ports/cortex-m/src/context.rs`](ports/cortex-m/src/context.rs) `switch_context`：提供 `pub unsafe extern "C" fn __caplet_switch(old_sp: *mut u8, new_sp: *mut u8)` 汇编实现（保存/恢复 r4-r11 + sp + lr）。
- [ ] **0.2.4** PendSV handler：`ports/cortex-m/src/context.rs::pendsv_handler` 实现 `ICSR |= 1<<28` 触发，handler 内调 `__caplet_switch`，异常现场符合 AAPCS。
- [ ] **0.2.5** 新增 `ports/cortex-m/src/interrupt.rs` `enable_pendsv()` / `pend_pendsv()`，用 `cpsie i` / `movw+movt+str` 内联。
- [ ] **0.2.6** 单测：host-side `tests/scheduler_unit.rs` —— mock 时钟走 5 tick，断言两个任务的执行次数比例。

### 0.3 真实 syscall handler（`crates/caplet-kernel/src/{syscall_cap,syscall_task,syscall_ipc}.rs`）

- [ ] **0.3.1** `syscall_cap.rs` 五个 handler 全部接通 CSpace：
  - `CapDerive` → `cspace.allocate(parent, attrs)` 真实派生
  - `CapDuplicate` → 引用计数 +1
  - `CapRevoke` → `revocation::revoke(handle, mode)`
  - `CapRevokeSelf` → 调用方 current task CSpace
  - `CapGrant` → 拷贝到 target CSpace（验证 cap 在当前 task 持有）
  - `CapFind` → 走 [`cap_set.rs`](crates/caplet-cap/src/cap_set.rs) `CapSet::lookup`
- [ ] **0.3.2** `syscall_task.rs` 五个 handler 接通 scheduler：
  - `TaskSpawn` → `TaskBuilder::build()` + `ready_queue.push()`
  - `TaskYield` → `Scheduler::yield_current()`
  - `TaskExit` → 标记 `Terminated` + 回收 CSpace
  - `TaskSleep` → 进 timer wheel
  - `TaskWake` → 出 timer wheel 进 ready_queue
- [ ] **0.3.3** `syscall_ipc.rs` 六个 handler 接通 [`SyncChannel`](crates/caplet-sync/src/sync_channel.rs)：
  - `IpcSend` / `IpcRecv` / `IpcCall` / `IpcReply` / `EndpointCreate` / `EndpointClose`
- [ ] **0.3.4** 引入 [`crates/caplet-kernel/src/syscall_dispatch.rs`](crates/caplet-kernel/src/syscall_dispatch.rs) 错误码统一映射（`CapError` / `TaskError` / `IpcError` → `SyscallResult::err`）。
- [ ] **0.3.5** 重写 [`crates/caplet-kernel/src/syscall_dispatch.rs`](crates/caplet-kernel/src/syscall_dispatch.rs) 真实 syscall 入口（在 SVC 异常 handler 里调用，与 PendSV 同位置 `ports/cortex-m/src/context.rs`）。
- [ ] **0.3.6** 安全位（§5.8）在 handler 内硬检查：`audit` / `infect` / `persistent` / `transferable`。

### 0.4 Cap → 资源真实桥接（`crates/caplet-cap/src/cap.rs` + `cspace.rs`）

- [ ] **0.4.1** 重写 [`crates/caplet-cap/src/cap.rs`](crates/caplet-cap/src/cap.rs) `Cap::get/get_mut`：从 `Handle` 解析 `SlotMetadata` → 调用 `cspace.with_handle()` 借出 `&T`，绑定 lifetime。
- [ ] **0.4.2** 实现 `Drop`：调 `cap_revoke_self()` 真正回收 slot（走 [`revocation.rs`](crates/caplet-cap/src/revocation.rs)）。
- [ ] **0.4.3** [`crates/caplet-cap/src/cspace.rs`](crates/caplet-cap/src/cspace.rs) `revoke()`：补上 epoch bump + 遍历 `DerivationNode.child_count` 递归回收（实现慢路径）。
- [ ] **0.4.4** [`crates/caplet-cap/src/revocation.rs`](crates/caplet-cap/src/revocation.rs) `revoke_slow`：真实遍历树，对每个 leaf slot 调 cleanup callbacks，再标 invalid。
- [ ] **0.4.5** [`crates/caplet-kernel/src/slot_allocator.rs`](crates/caplet-kernel/src/slot_allocator.rs) `free()` 真实回收：把 slot index 写回 free_bitmap。

### 0.5 宏真正生成代码（`crates/caplet-macros/src/`）

- [ ] **0.5.1** [`crates/caplet-macros/src/task_macro.rs`](crates/caplet-macros/src/task_macro.rs)：解析 `priority`、`stack`、`caps`、`cpu_quota`、`mem_budget`、`message_quota` 字段，每个字段缺失给默认值；输出形如：

```rust
#[allow(non_camel_case_types)]
struct <fn_name>_Task;
impl caplet_task::TaskConfig for <fn_name>_Task {
    const PRIORITY: u8 = <priority>;
    const STACK_BYTES: usize = <stack>;
    // ...
}
```

并用 `#[distributed_slice]` 或 `linkme` 风格的静态数组在 link 期聚合。

- [ ] **0.5.2** [`crates/caplet-macros/src/main_macro.rs`](crates/caplet-macros/src/main_macro.rs)：生成 `#[no_mangle] pub extern "C" fn caplet_kernel_entry() -> !`，构造 `Kernel::start(main_async_fn)`。
- [ ] **0.5.3** 引入 `linkme` 依赖到 `caplet-task/Cargo.toml`，把宏生成的静态表聚合。

### 0.6 真实启动路径（`boards/hello-qemu-cm{3,4}/src/main.rs`）

- [ ] **0.6.1** 把 [`boards/hello-qemu-cm3/src/main.rs`](boards/hello-qemu-cm3/src/main.rs) / [`boards/hello-qemu-cm4/src/main.rs`](boards/hello-qemu-cm4/src/main.rs) 重写为：

```rust
#[caplet::main]
async fn main(spawner: Spawner, caps: Caplets) -> ! {
    let uart = caps.service("uart:1").unwrap();
    spawner.spawn(hello_task(uart)).unwrap();
    spawner.spawn(control_loop(uart)).unwrap();
}

#[caplet::task(priority = 3, stack = 1024, caps = [Uart1])]
async fn hello_task(uart: Cap<Uart1>) {
    uart.write_all(b"hello world\n").await.unwrap();
}
```

- [ ] **0.6.2** 把 `cortex-m-rt` 的 `#[entry]` + `#[cortex_m_rt::interrupt]` 接入 [`ports/cortex-m/src/boot.rs`](ports/cortex-m/src/boot.rs)，删 boards 的自实现 `rust_main`。
- [ ] **0.6.3** 引入 `caplet-cortex-m` 的 link.x 替换 boards 各自的链接脚本差异；保留 mps2-an385 / mps2-an500 内存模型。
- [ ] **0.6.4** 删除 boards 中重复的 `#[panic_handler]` 与 port 中 `cortex_m_panic` 冲突，统一用 `cortex-m-rt` 自带的 panic_runtime。

### 0.7 全局状态与日志清理

- [ ] **0.7.1** [`crates/caplet-log/src/lib.rs`](crates/caplet-log/src/lib.rs) `log()`：把 `static mut LOG_LEVEL` 改为 `AtomicU8`；根据 level 走 `SemihostingWriter` 或 `UartWriter`（后者注册一个 `Cap<Uart>` backend）。
- [ ] **0.7.2** [`crates/caplet-kernel/src/kernel_init.rs`](crates/caplet-kernel/src/kernel_init.rs) 的 `static mut KERNEL_INSTANCE` 改为 `AtomicPtr<Kernel>` + 初始化 fence。
- [ ] **0.7.3** 把 [`crates/caplet-task/src/caplets.rs`](crates/caplet-task/src/caplets.rs) `service<T>()` 真正实现：读当前 task 的 `CSpace` → `CapSet::lookup(key)` → 返回 `Option<Handle<T>>`。

### 0.8 真实 Tickless 桩（QEMU 限定）

- [ ] **0.8.1** [`crates/caplet-hal/src/timer.rs`](crates/caplet-hal/src/timer.rs) 增 `set_next_deadline(ticks)` / `current_ticks()`。
- [ ] **0.8.2** [`ports/cortex-m/src/lib.rs`](ports/cortex-m/src/lib.rs) 新增 `systick.rs`，封装 SysTick reload + `WFI` 内联。
- [ ] **0.8.3** `crates/caplet-kernel/src/main_task.rs` `idle_task`：在没有 ready task 时，调 `systick::compute_next_deadline()` 后 `wfi`；被中断唤醒后回来检查 wake_queue。

---

## Phase 1 — M2-A：Kani 形式化 + syscall 矩阵

> 完成 docv1 §10.3 与 §12 M2 第一条

### 1.1 Kani 接入

- [ ] **1.1.1** 根 `Cargo.toml` 添加 `dev-dependencies`：`cargo-kani` 不放在 workspace，由 `.github/workflows/ci.yml` 装 `cargo-kani`。
- [ ] **1.1.2** `.cargo/config.toml` 增加 `[target.'cfg(kani)']` 用于在 Kani 下调整依赖。
- [ ] **1.1.3** 新建 `tools/kani/`：每个核心函数一个 harness。
- [ ] **1.1.4** CI：`kani-syscall` job，install `cargo-kani`，跑 `cargo kani --harness ...`。

### 1.2 Syscall 调度矩阵 Kani 覆盖

> 全部 25 个 syscall 都接 Kani harness。

- [ ] **1.2.1** Capability 类（5）—— `tools/kani/syscall_cap.rs`：
  - `CapDerive` harness：枚举父权限 {R, RW, RWX} × 子衰减策略；穷举 parent handle 状态（valid/revoked/never）。
  - `CapDuplicate`：原子化引用计数上溢检查（harness 给 refcount ≥ MAX-1）。
  - `CapRevoke` 快路径：epoch 单调性；harness bound 3 个并发 slot。
  - `CapRevokeSelf`：current task 拥有 precondition。
  - `CapGrant`：transferable 安全位为 false 时必须拒绝；harness 覆盖两种状态。
  - `CapFind`：CapSet lookup 无 panics + key 长度上限。
- [ ] **1.2.2** Task 类（5）—— `tools/kani/syscall_task.rs`：
  - `TaskSpawn`：priority 在 [0, MAX) 才生效；stack 满足栈对齐 8B。
  - `TaskYield`：current state ∈ {Ready, Running} 才成功。
  - `TaskExit`：transition 到 Terminated 不丢审计事件。
  - `TaskSleep` / `TaskWake`：timer wheel 顺序保持（harness 注入随机 offset）。
- [ ] **1.2.3** IPC 类（6）—— `tools/kani/syscall_ipc.rs`：
  - `IpcSend` 端点 closed 时返回 `EndpointClosed`；bounded channel 满时返回 `WouldBlock`。
  - `IpcRecv` 在 closed 端点不挂死；返回 NotFound 而非 panic。
  - `IpcCall`：远程 task 已 terminated 时返回 `NoPeer`，不悬挂。
  - `IpcReply`：reply 后调用方状态机合法（仅允许 once）。
  - `EndpointCreate` / `EndpointClose`：handle 生命周期正确。
- [ ] **1.2.4** Memory / FS / Net / Misc 类（9）—— 拆分到 `tools/kani/syscall_memory.rs`、`syscall_fs.rs`、`syscall_net.rs`、`syscall_misc.rs`：
  - 内存映射：不出 `[base, base+size)`，不重叠。
  - FS open / read / write 路径不 panic。
  - Socket bind / connect / send / recv 在假 socket 上不越界。
  - `GetTime` / `Yield` / `Log` / `Panic` 四类返回确定性。
- [ ] **1.2.5** `tools/kani/run_all.sh`：顺序执行 25 个 harness，输出汇总；CI 报告 `cargo kani --output-format=terse` 摘要。

### 1.3 其他 Kani 任务

- [ ] **1.3.1** `tools/kani/derivation.rs`：`cap_derive_with_options` 在所有 5 个维度（权限 / 范围 / 次数 / TTL / 对象限定）下都能拒绝非法派生。
- [ ] **1.3.2** `tools/kani/priority_ceiling.rs`：天花板协议数学正确性（无环、无死锁）。
- [ ] **1.3.3** `tools/kani/audit.rs`：审计事件序号单调。

---

## Phase 2 — M2-B：HAL trait + STM32F4 / nRF52 实现

### 2.1 HAL trait 完善（`crates/caplet-hal/`）

- [ ] **2.1.1** [`crates/caplet-hal/src/uart.rs`](crates/caplet-hal/src/uart.rs) 加 `read_timeout`、`flush_tx`、error 分类 `Overrun / Framing / Parity`。
- [ ] **2.1.2** [`crates/caplet-hal/src/gpio.rs`](crates/caplet-hal/src/gpio.rs) 加 `into_input/output/open_drain`、中断 `into_irq<F>` 关联 [`crates/caplet-hal/src/interrupt.rs`](crates/caplet-hal/src/interrupt.rs) 的 `IrqHandler`。
- [ ] **2.1.3** [`crates/caplet-hal/src/i2c.rs`](crates/caplet-hal/src/i2c.rs) 加 DMA + 错误 `BusBusy / ArbitrationLost / Nack`。
- [ ] **2.1.4** [`crates/caplet-hal/src/spi.rs`](crates/caplet-hal/src/spi.rs) 加 mode 0..3 显式枚举、`cs::ActiveLow` 模板。
- [ ] **2.1.5** [`crates/caplet-hal/src/timer.rs`](crates/caplet-hal/src/timer.rs) 加 `oneshot` / `periodic` 两种模式、deadline API（M2.6 用）。
- [ ] **2.1.6** [`crates/caplet-hal/src/interrupt.rs`](crates/caplet-hal/src/interrupt.rs) 加 `IrqHandler::register` / `enable` / `disable` / `set_priority`。
- [ ] **2.1.7** [`crates/caplet-hal/src/lib.rs`](crates/caplet-hal/src/lib.rs) 统一 `HalError` 错误码映射到 `CapError`。

### 2.2 HAL SemVer 与版本门（§13.5）

- [ ] **2.2.1** `crates/caplet-hal/Cargo.toml` 锁定 major 版本 `0.2.0`，README 写明 SemVer 规则。
- [ ] **2.2.2** `tools/hal-migrate/`：占位 crate 写明 v0.1 → v0.2 迁移流程（M3 实现）。

### 2.3 STM32F4 HAL 实现（`hal/hal-stm32/`）

> 新建 crate

- [ ] **2.3.1** 新建 `hal/hal-stm32/Cargo.toml`：依赖 `cortex-m`、`stm32f4xx-hal`（PAC 兼容层）、`caplet-hal = 0.2`。
- [ ] **2.3.2** 实现 `Uart` for `stm32f4::Usart1/2/3`（USART1 → PC UART，USART2 → mps2-an385 上的 UART0）。
- [ ] **2.3.3** 实现 `GpioPin` for `stm32f4::Pin<P, MODE>`：含 `set / clear / toggle / input_pullup`。
- [ ] **2.3.4** 实现 `I2c` for `stm32f4::I2c1/2`，含 start/stop/restart、ACK/NACK。
- [ ] **2.3.5** 实现 `Spi` for `stm32f4::Spi1/2/3`。
- [ ] **2.3.6** 实现 `Timer` for `stm32f4::Tim2`（32-bit，定时器做 deadline）。
- [ ] **2.3.7** 实现 `IrqHandler` for `stm32f4::NVIC`，封装 `cortex_m::interrupt::free`。
- [ ] **2.3.8** 单测：host-side 用 `stm32f4xx-hal` 的 mock（如果有）；否则 QEMU stm32 仿真跑 `boards/stm32f4-discovery` smoke。
- [ ] **2.3.9** README 写明 pinout 表（F446ZE 引脚 → USART / I2C / SPI / TIM 映射）。

### 2.4 nRF52 HAL 实现（`hal/hal-nrf52/`）

> 新建 crate

- [ ] **2.4.1** 新建 `hal/hal-nrf52/Cargo.toml`：依赖 `nrf52840-hal`（PCA10056）/ `nrf52833-hal`，`caplet-hal = 0.2`。
- [ ] **2.4.2** 实现 `Uart` for nRF52 `Uarte` peripheral（含 EasyDMA）。
- [ ] **2.4.3** 实现 `GpioPin` for nRF52 `Gpio`。
- [ ] **2.4.4** 实现 `Spi` for `SpiMaster`（含 EasyDMA）。
- [ ] **2.4.5** 实现 `I2c` for `Twi`（含 EasyDMA）。
- [ ] **2.4.6** 实现 `Timer` for `Timer0/1/2`（RTC1 用作 system tick；TIMER 通用 API）。
- [ ] **2.4.7** 实现 `IrqHandler` for nRF52 NVIC。
- [ ] **2.4.8** README 写明 pinout 表（DK 板）。

### 2.5 安全关键外设（§9.3）

- [ ] **2.5.1** `hal/hal-stm32/src/crypto.rs`：STM32 CRC / AES / TRNG 包装为 `Cap<CryptoKey, {1-time}>`，用完撤销。
- [ ] **2.5.2** `hal/hal-nrf52/src/quaddec.rs`：QDEC 包装为 `Cap<QdecOnly>`。
- [ ] **2.5.3** 创建 `crates/caplet-hal/src/security.rs`：定义 `Cap<CryptoKey>` / `Cap<FlashWrite>` / `Cap<JtagDebug>` 的安全约束。

---

## Phase 3 — M2-C：Nucleo-F446ZE 板级支持

### 3.1 Board crate

- [ ] **3.1.1** 新建 `boards/nucleo-f446ze/Cargo.toml`：依赖 `cortex-m-rt`、`hal-stm32`、`caplet-kernel`、`caplet-app`。
- [ ] **3.1.2** 新建 `boards/nucleo-f446ze/memory.x`：F446ZE 内存模型（FLASH=0x08000000 512K, RAM=0x20000000 128K）。
- [ ] **3.1.3** `boards/nucleo-f446ze/src/main.rs`：`#[caplet::main]` 走 HAL STM32F4，DI 例程 `led_blink` 写 STM32 LD1（PB0）。
- [ ] **3.1.4** `boards/nucleo-f446ze/board.toml`：board config（USART3 → ST-Link VCP 引到 host PC）。
- [ ] **3.1.5** `.cargo/config.toml` runner 配置：`probe-rs run --chip STM32F446ZETx`。

### 3.2 CI 与冒烟

- [ ] **3.2.1** `.github/workflows/ci.yml` `build-f446ze` job：只 build 不烧（无 probe-rs runner 时 skip）。
- [ ] **3.2.2** `.github/workflows/hardware-smoke.yml`（手动触发，`workflow_dispatch`）：用 QEMU stm32f4-discovery 仿真跑 `led_blink` —— 验证启动路径。
- [ ] **3.2.3** `tests/qemu-stm32f4/run.sh`：跟 `qemu-cm3/run.sh` 结构一致。

### 3.3 GD32 预留

- [ ] **3.3.1** 在 `hal/` 下加 `hal-gd32/Cargo.toml` 占位 + README，标记 M3 实现。
- [ ] **3.3.2** 提供 GD32F303 / GD32VF103 pin 兼容说明。

---

## Phase 4 — M2-D：资源预算系统（§6.4）

### 4.1 任务配额抽象

- [ ] **4.1.1** [`crates/caplet-task/src/task_config.rs`](crates/caplet-task/src/task_config.rs) 增字段：`memory_budget: usize`、`message_quota: u32`、`cpu_quota_per_epoch: u32`。
- [ ] **4.1.2** 新建 `crates/caplet-kernel/src/quota.rs`：`QuotaSet` / `QuotaTracker`，维护当前 task 的实时用量。
- [ ] **4.1.3** 在 [`crates/caplet-kernel/src/scheduler.rs`](crates/caplet-kernel/src/scheduler.rs) `schedule()` 入口检查 `current.quota.cpu_per_epoch`；超限则降优先级或挂起到下一 epoch。
- [ ] **4.1.4** syscall 在 `CapDerive` 后更新 `current.quota.memory`；超 `memory_budget` 拒绝并审计。
- [ ] **4.1.5** IPC `IpcSend` 后更新 sender 的 `message_quota`；超限返回 `OverQuota`。
- [ ] **4.1.6** epoch 概念（每 1000 ms 重置 cpu 配额）。

### 4.2 预算拒绝与事件

- [ ] **4.2.1** `crates/caplet-task/src/caplets.rs` `OverBudget(task_id, kind)` 事件发布。
- [ ] **4.2.2** `crates/caplet-sync/src/event.rs` 接通：事件在 ready_queue 注入即可被监听任务 `#[caplet::task]` 收到。
- [ ] **4.2.3** Kani harness：`tools/kani/quota.rs` —— 任何 `QuotaSet::update` 都不会下溢或上溢。

---

## Phase 5 — M2-E：Tickless Idle（§6.4）

### 5.1 算法

- [ ] **5.1.1** `crates/caplet-kernel/src/timer_wheel.rs`：新增 timer wheel（4 桶 × 256 tick，覆盖 0..2^16 tick）。
- [ ] **5.1.2** `Scheduler::compute_idle_deadline()`：扫 timer wheel + sleep queue，返回最近 deadline。
- [ ] **5.1.3** SysTick 重载：从 1ms tick 改为 variable reload，仅在 new deadline 后重置。
- [ ] **5.1.4** [`ports/cortex-m/src/systick.rs`](ports/cortex-m/src/systick.rs) 中 `SysTick::reload(deadline_ticks)`。

### 5.2 Idle 主体

- [ ] **5.2.1** `crates/caplet-kernel/src/main_task.rs` `idle_task`：开 `cpsie i` → 调 `Scheduler::compute_idle_deadline()` → `systick::reload` → `wfi` → 中断返回后 `Scheduler::tick()`。
- [ ] **5.2.2** 多就绪任务时保持 tickless；所有任务进入 sleep 才走 WFI。

### 5.3 验证

- [ ] **5.3.1** `tests/qemu-cm4-tickless/run.sh`：在 mps2-an500 上跑 1000 虚拟毫秒，断言 SysTick 中断次数下降到 ≤ N（N 与 deadline 间隔成反比）。
- [ ] **5.3.2** `crates/caplet-kernel/benches/tickless_bench.rs`（可选）。

---

## Phase 6 — M2-F：VFS + LittleFS + FAT32

### 6.1 VFS crate

- [ ] **6.1.1** 新建 `crates/caplet-fs/Cargo.toml`：`defines_fs_trait` 与 `VfsTable`。
- [ ] **6.1.2** `crates/caplet-fs/src/vfs.rs`：`trait Vfs { open / stat / read / write / close / mkdir / unlink / readdir }`。
- [ ] **6.1.3** `crates/caplet-fs/src/cap.rs`：`Cap<MountPoint>` / `Cap<File>` / `Cap<Directory>` 类型。
- [ ] **6.1.4** `crates/caplet-fs/src/syscall_fs.rs`：`FsOpen` / `FsClose` / `FsRead` / `FsWrite` / `FsStat` 五个 syscall handler。
- [ ] **6.1.5** 在 [`crates/caplet-kernel/src/syscall.rs`](crates/caplet-kernel/src/syscall.rs) 加 `SyscallOp` 枚举这五个值。

### 6.2 LittleFS 后端（`crates/caplet-fs-lfs/`）

- [ ] **6.2.1** 新建 `crates/caplet-fs-lfs/Cargo.toml`：vendor `littlefs = { version = "0.6", default-features = false }`。
- [ ] **6.2.2** `crates/caplet-fs-lfs/src/backend.rs`：`impl littlefs::Storage for FlashBackend`，接受 `&[HalFlashDriver]`。
- [ ] **6.2.3** `crates/caplet-fs-lfs/src/fs.rs`：impl `Vfs` for `LfsWrapper`。
- [ ] **6.2.4** 集成 `crates/caplet-fs-lfs/tests/powercut.rs`：随机 fault inject（write 中途重启）验证 metadata 恢复。
- [ ] **6.2.5** README：API 范例 + 注意（小 flash 区域最佳实践）。

### 6.3 FAT32 后端（`crates/caplet-fs-fat/`）

- [ ] **6.3.1** 新建 `crates/caplet-fs-fat/Cargo.toml`：用 `embedded-fat = "0.3"` + 自实现 block device 抽象。
- [ ] **6.3.2** `crates/caplet-fs-fat/src/blockdev.rs`：`trait BlockDevice<SIZE>` for `SdCardHalBlockDev`。
- [ ] **6.3.3** `crates/caplet-fs-fat/src/fs.rs`：impl `Vfs` for `Fat32Wrapper`。
- [ ] **6.3.4** `crates/caplet-fs-fat/tests/integration.rs`：放一个预制 fat32 image 进 build tree。
- [ ] **6.3.5** README：API + 兼容性测试说明。

### 6.4 RAMFS 调试后端（docv1 §15.3）

- [ ] **6.4.1** 新建 `crates/caplet-fs-ramfs/Cargo.toml`。
- [ ] **6.4.2** `crates/caplet-fs-ramfs/src/fs.rs`：内存 btree 文件表 + impl Vfs。
- [ ] **6.4.3** 行为测试：`crates/caplet-fs-ramfs/tests/persist.rs` 在进程崩溃后恢复一致。

### 6.5 boot 时挂载

- [ ] **6.5.1** [`crates/caplet-kernel/src/boot.rs`](crates/caplet-kernel/src/boot.rs) `boot_task_main()`：解析 `board.toml` 的 `mount` 段，注册每个 mount 到 VFS table，构造对应 `Cap<MountPoint>` 注入主任务 CSpace。
- [ ] **6.5.2** `tests/qemu-cm4-fs/run.sh` 跑 mount + write + read + reopen 流程，断言文件存在 + 内容一致。

---

## Phase 7 — M2-G：网络协议栈（LWIP 风格）

### 7.1 网络架构

- [ ] **7.1.1** 新建 `crates/caplet-net/Cargo.toml`：抽象 `trait NetworkInterface`（§16.2）。
- [ ] **7.1.2** `crates/caplet-net/src/iface.rs`：`NetworkInterface` trait + `FrameBuf` 类型。
- [ ] **7.1.3** `crates/caplet-net/src/socket.rs`：`Cap<Socket>`（UDP/TCP/RAW） + syscall `NetBind`/`NetConnect`/`NetSend`/`NetRecv`/`NetClose`。
- [ ] **7.1.4** `crates/caplet-net/src/rx_ring.rs`：DMA 接收环形 buffer（`Cap<SharedRing>`）。
- [ ] **7.1.5** `crates/caplet-kernel/src/syscall_net.rs` 接通 net syscall 到 `crates/caplet-kernel/src/syscall.rs`。

### 7.2 协议栈实现（自研，LWIP 风格）

- [ ] **7.2.1** 新建 `crates/caplet-eth/src/mac.rs`：Ethernet II 帧 parse/encode（不含 PHY state machine，HAL 抽象）。
- [ ] **7.2.2** 新建 `crates/caplet-eth/src/arp.rs`：ARP 请求/应答缓存（最长 5 分钟）。
- [ ] **7.2.3** 新建 `crates/caplet-ip/src/ipv4.rs`：IP 头 / 分片 / 重装配 / 校验。
- [ ] **7.2.4** 新建 `crates/caplet-icmp/src/icmp.rs`：echo req/resp。
- [ ] **7.2.5** 新建 `crates/caplet-udp/src/udp.rs`：IPv4/UDP。
- [ ] **7.2.6** 新建 `crates/caplet-tcp/src/`：`tcp_state.rs`（CLOSED/LISTEN/SYN-SENT/...）、`rtt.rs`、`cwnd.rs`（NewReno）、`reassembly.rs`。
- [ ] **7.2.7** 新建 `crates/caplet-dhcp/src/`：v4 client（DISCOVER/OFFER/REQUEST/ACK）。
- [ ] **7.2.8** 新建 `crates/caplet-dns/src/`：极简 A/AAAA query。

### 7.3 PHY HAL

- [ ] **7.3.1** `crates/caplet-hal/src/ethernet.rs`：定义 `EthernetPhy { init / poll_link / send_frame / recv_frame }`。
- [ ] **7.3.2** `hal/hal-stm32/src/eth.rs`：DP83848 PHY + MAC 集成（依赖 `stm32-eth`）。
- [ ] **7.3.3** `tests/qemu-net-stack/run.sh`：QEMU -net user + 启用 slirp，让 demo 走真实 TCP echo。

### 7.4 Kani 覆盖（接 Phase 1）

- [ ] **7.4.1** `tools/kani/network.rs`：ARP 表无 panic / IP 选项不溢出 / TCP 序号单调 / DNS query 长度上限。

### 7.5 socket 能力集成（§16.4）

- [ ] **7.5.1** socket 可派生为 `ReadOnly` / `WriteOnly` / 限速（如每秒 ≤ N 帧）。
- [ ] **7.5.2** [`crates/caplet-net/src/socket.rs`](crates/caplet-net/src/socket.rs) Drop 时主动 close（关端口 + 释放 buffer）。
- [ ] **7.5.3** `crates/caplet-cap/src/derive_net.rs`：`NetCap::derive(attrs)` 接受 attenuation。

---

## Phase 8 — M2-H：工业 demo（传感器 + LittleFS + MQTT-SN）

### 8.1 工业协议

- [ ] **8.1.1** 新建 `crates/caplet-mqtt-sn/Cargo.toml`：实现 MQTT-SN v1.2（CONNECT/CONNACK/REGISTER/REGACK/PUBLISH/PUBACK/SUBSCRIBE/SUBACK/DISCONNECT）。
- [ ] **8.1.2** `crates/caplet-mqtt-sn/src/state.rs`：客户端状态机。
- [ ] **8.1.3** `crates/caplet-mqtt-sn/src/encoder.rs`：packet encode + length encoding。
- [ ] **8.1.4** `crates/caplet-mqtt-sn/src/discovery.rs`：gateway discovery。
- [ ] **8.1.5** `crates/caplet-mqtt-sn/src/will.rs`：last will。
- [ ] **8.1.6** README：与 `rumqttd` / Mosquitto 服务器的互通测试说明。

### 8.2 传感器抽象

- [ ] **8.2.1** 新建 `crates/caplet-sensor/Cargo.toml`：`trait Sensor { type Reading; async fn read(&mut self) -> Result<Self::Reading, SensorError> }`。
- [ ] **8.2.2** `hal/hal-stm32/src/sensors/tmp102.rs`：TMP102 / LM75 温感（I2C），impl `Sensor`。
- [ ] **8.2.3** `crates/caplet-sensor/src/mock.rs`：host/mock 实现，用于 QEMU demo。

### 8.3 工业 demo board

- [ ] **8.3.1** 新建 `boards/industrial-demo/Cargo.toml`：依赖 `hal-stm32` + `caplet-mqtt-sn` + `caplet-fs-lfs`。
- [ ] **8.3.2** `boards/industrial-demo/src/main.rs`：

```rust
#[caplet::main]
async fn main(spawner: Spawner, caps: Caplets) -> ! {
    let sensor = caps.service("i2c:1/tmp102:0").unwrap();
    let store  = caps.service("fs:lfs:/data").unwrap();
    let mqtt   = caps.service("net:mqtt-sn").unwrap();
    spawner.spawn(sample_loop(sensor, store));
    spawner.spawn(upload_loop(store, mqtt));
}
```

- [ ] **8.3.3** `boards/industrial-demo/src/sample_loop.rs`：每 1 s 读 TMP102，写 `/data/sensor-<ts>.bin` 含时间戳 + 温度 + sig。
- [ ] **8.3.4** `boards/industrial-demo/src/upload_loop.rs`：每 30 s 读 last 1 KB + publish 到 `mqtt://broker.local/tenant/caplet/temperature`，带 last will `offline`。
- [ ] **8.3.5** `boards/industrial-demo/board.toml`：I2C1 → PB8/PB9，sensor addr=0x48；ETH RMII → LAN8742；CS QIO=22 MBit；LittleFS 1 MB on internal flash。
- [ ] **8.3.6** `boards/industrial-demo/.cargo/config.toml`：runner 复用 QEMU stm32 net。

### 8.4 端到端验证

- [ ] **8.4.1** `tests/industrial-demo/run.sh`：起 `rumqttd` 本地 server + QEMU industrial demo，断言 sensor 文件被写入 + MQTT 收到数据。
- [ ] **8.4.2** `docs/industrial-demo.md`：步骤详解。

### 8.5 CI 集成

- [ ] **8.5.1** `.github/workflows/ci.yml` 新增 `industrial-demo` job，跑 `tests/industrial-demo/run.sh`。
- [ ] **8.5.2** 截图/输出归档到 `tests/artifacts/industrial-demo/<sha>/`。

---

## Phase 9 — 文档 + 验证（贯穿）

### 9.1 每个 crate README

> 全部按 docv1 §21.1 要求。每个 crate 至少要含：目的、对外 API 摘要、最小例子、safety 策略、已知限制。

- [ ] **9.1.1** `crates/caplet-kernel/README.md`：kernel API、unsafe 边界、调度策略。
- [ ] **9.1.2** `crates/caplet-cap/README.md`：能力模型、派生示例、安全位说明。
- [ ] **9.1.3** `crates/caplet-task/README.md`：TaskBuilder 用法、生命周期、caplets。
- [ ] **9.1.4** `crates/caplet-async/README.md`：Executor 模型、waker 与 wake_queue 关系。
- [ ] **9.1.5** `crates/caplet-sync/README.md`：自旋/Mutex 与 stackless async 协作使用规范。
- [ ] **9.1.6** `crates/caplet-hal/README.md`：HAL SemVer、迁移指南。
- [ ] **9.1.7** `crates/caplet-log/README.md`：backend 接入。
- [ ] **9.1.8** `crates/caplet-macros/README.md`：宏参数表。
- [ ] **9.1.9** `ports/cortex-m/README.md`：Cortex-M 移植步骤。
- [ ] **9.1.10** `hal/hal-stm32/README.md` + `hal/hal-nrf52/README.md`。
- [ ] **9.1.11** `boards/hello-qemu-cm3/README.md` + `boards/hello-qemu-cm4/README.md` + `boards/nucleo-f446ze/README.md` + `boards/industrial-demo/README.md`。

### 9.2 ADR 文档（docv1 §21.1）

- [ ] **9.2.1** `docs/adr/0001-capability-as-safety-primitive.md`：选能力而非 ACL/kobject。
- [ ] **9.2.2** `docs/adr/0002-stackless-async-default.md`：选 stackless 默认，stackful/coroutine 备选。
- [ ] **9.2.3** `docs/adr/0003-epoch-revocation-fast-path.md`：双轨撤销的快路径选择。
- [ ] **9.2.4** `docs/adr/0004-kernel-error-encoding.md`：统一错误模型。
- [ ] **9.2.5** `docs/adr/0005-hal-semver-migration.md`：HAL SemVer 与迁移脚本。
- [ ] **9.2.6** `docs/adr/0006-quota-policy-and-limits.md`：memory / cpu / message 配额策略。
- [ ] **9.2.7** `docs/adr/0007-network-stack-from-scratch.md`：自研 LWIP 风格，不依赖 lwip / smoltcp 的核心理由。

### 9.3 安全与开发文档

- [ ] **9.3.1** `docs/security-model.md`：TCB 边界、安全位定义、审计日志格式。
- [ ] **9.3.2** `docs/porting-guide.md`：新芯片移植步骤（基于 cortex-m-rt 框架）。
- [ ] **9.3.3** `docs/dev-quickstart.md`：5 分钟本地跑 hello-world（已存在，需更新到 Phase 0.6 后状态）。
- [ ] **9.3.4** `docs/kani-guide.md`：Kani harness 写作指南。
- [ ] **9.3.5** `docs/industrial-demo.md`：从硬件到 broker 完整流程（已列于 Phase 8.4）。

### 9.4 CI 全绿门禁

> docv1 §10.2 / §10.3 要求 + §21.3 + §25.11

- [ ] **9.4.1** `.github/workflows/ci.yml` 增加步骤：`cargo clippy --all-targets -- -D warnings -W clippy::pedantic -A clippy::missing_errors_doc`；`#![forbid(unsafe_code)]` lint（仅应用 crate）。
- [ ] **9.4.2** grep `# SAFETY` 在 `unsafe` 块附近 5 行的存在性检查（基于 `cargo-geiger` + 自写 script）。
- [ ] **9.4.3** `cargo fmt --check` 强制。
- [ ] **9.4.4** `cargo deny check` 检查依赖 license / advisory。
- [ ] **9.4.5** Phase 1 Kani job（独立 runner）。
- [ ] **9.4.6** 五个 QEMU 集成 job 全部 `grep` 断言各自 demo 字符串。
- [ ] **9.4.7** `cargo bench` 跑 §20 性能回归（`context_switch_bench` 等），输出 SVG，保存到 `tests/bench-results/`。
- [ ] **9.4.8** 在 PR 模板里强制填「Kani 检查」「QEMU 检查」「cap_docs 更新」三栏。

---

## Phase 10 — 风险与兜底

### 10.1 路径依赖风险

- [ ] **10.1.1** 当 Phase 0 任一项无法闭合（例如 `linkme` 与 `cortex-m-rt` 兼容性问题）→ 在该处插一个 fallback：暂时延后宏代码生成，先手动注册 task table；记录为 ADR。
- [ ] **10.1.2** Kani 在 `dhcp.rs` 上爆炸（state space 过大）→ 把 `DhcpState` 缩到 3 个状态 + harness 拆 harness per state。
- [ ] **10.1.3** 自研 TCP 引擎无法在 M2 内完整 NewReno → 退回 RFC793 + simple_timeout，作为最小可用；NewReno 留 M3。

### 10.2 性能门（§20）

- [ ] **10.2.1** `context_switch_bench`：在 mps2-an500 上测得 < 2 µs 才算 M2 达成；否则插入 IPC cache line padding 重测。
- [ ] **10.2.2** `cap_derive_bench`：快路径 < 100 ns（mps2-an500）。
- [ ] **10.2.3** `ipc_send_bench`：< 1 µs。

### 10.3 安全门

- [ ] **10.3.1** Kani 全部 25 syscall + 4 个 derivation/priority-ceiling/audit harness CI 全绿。
- [ ] **10.3.2** 工业 demo 上 Mqtt-SN 的 QoS=1 publish 在断网容错 30 s 内成功重传。

---

## 附录 A — 文件结构计划（增量）

```
CapletRT/
├── crates/
│   ├── caplet-{kernel,cap,task,async,sync,log,macros,hal}        # M1 已存在，Phase 0 补全
│   ├── caplet-fs/                                                # Phase 6
│   ├── caplet-fs-{lfs,fat,ramfs}/                                # Phase 6
│   ├── caplet-net/                                               # Phase 7
│   ├── caplet-{eth,ip,icmp,udp,tcp,dhcp,dns,mqtt-sn,sensor}/     # Phase 7-8
├── ports/cortex-m/                                                # Phase 0.2 / 0.6
├── hal/
│   ├── hal-stm32/                                                 # Phase 2.3
│   ├── hal-nrf52/                                                 # Phase 2.4
│   └── hal-gd32/                                                  # 占位 Phase 3.3
├── boards/
│   ├── hello-qemu-cm{3,4}/                                        # Phase 0.6 重写
│   ├── nucleo-f446ze/                                             # Phase 3
│   └── industrial-demo/                                           # Phase 8
├── tests/
│   ├── qemu-cm{3,4,industrial,net,fs}/                            # 各阶段
│   └── artifacts/                                                 # 输出归档
├── tools/
│   ├── kani/                                                       # Phase 1
│   ├── hal-migrate/                                                # Phase 2.2 占位
│   └── stack-analyzer/                                             # M3 占位
├── docs/
│   ├── adr/                                                       # Phase 9.2
│   ├── security-model.md                                          # Phase 9.3
│   ├── porting-guide.md                                           # Phase 9.3
│   ├── dev-quickstart.md                                          # 更新
│   ├── kani-guide.md                                              # Phase 9.3
│   └── industrial-demo.md                                         # Phase 8.4
└── .github/workflows/ci.yml + hardware-smoke.yml                  # 全程更新
```

---

## 附录 B — 完整 todo 数

| Phase | 内容 | todo 数 |
|---|---|---|
| 0 | M1 硬伤收尾 | 33 |
| 1 | Kani syscall 矩阵 | 12 |
| 2 | HAL trait + STM32F4 / nRF52 | 32 |
| 3 | Nucleo-F446ZE | 10 |
| 4 | 资源预算 | 9 |
| 5 | Tickless idle | 8 |
| 6 | VFS + LittleFS + FAT32 | 24 |
| 7 | 网络栈（LWIP 风格） | 23 |
| 8 | 工业 demo | 17 |
| 9 | 文档 + CI | 30 |
| 10 | 风险兜底 | 9 |
| **总计** | | **~207 todos** |

每个 todo 在 PR 标题里以 `[P0.1.1]`、`[P2.3.2]`、`[P8.4.1]` 风格编号，便于一次 PR 合并一个 phase 或一个 phase 子集。

---

## 附录 C — 执行节奏建议（10 个月，2026 Q3 至 2027 Q1）

| 月份 | 主要工作 |
|---|---|
| M+0 (8 月) | Phase 0 全量 + Phase 1.1/1.2 |
| M+1 (9 月) | Phase 1.3 + Phase 9.1 文档骨架 |
| M+2 (10 月) | Phase 2 (HAL) + Phase 6.1 (VFS) |
| M+3 (11 月) | Phase 3 (Nucleo) + Phase 6.2/6.3 (LFS/FAT) + Phase 6.4/6.5 |
| M+4 (12 月) | Phase 4 (quota) + Phase 5 (tickless) |
| M+5 (1 月) | Phase 7 网络栈 (eth/ip/icmp/udp/tcp + dhcp/dns) |
| M+6 (2 月) | Phase 7 (mqtt-sn/sensor) + Phase 8 (工业 demo) |
| M+7 (3 月) | Phase 9 ADR / 文档 / CI 全绿 + Phase 10 性能门 |
| **交付** | M1 收尾 + M2 全量，所有验收门通过 |

---

## 附录 D — 不在本计划范围（M3+）

- 多核 SMP / Cortex-A 真 MMU
- 动态加载 ELF / `.capp`
- 跨能力命名空间 IPC
- RISC-V port
- OTA / 安全启动
- HSM / 加密库深度集成

按设计文档 §12，这些是 M3（2027 Q2）起的话题。在 Phase 7 / 8 完成后，单独做 release tag `v1.0-m2`，将对应状态写入 `docv1.md` 的 release notes。