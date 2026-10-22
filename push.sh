#!/usr/bin/env bash
# Create logically scoped release commits for the completed CapletRT work.
# Each commit describes the behaviour or feature being delivered; file lists are
# kept as explicit arguments so nothing is discovered or added in bulk.
#
# Commit without pushing:
#   NO_PUSH=1 ./push.sh
# Commit and push:
#   ./push.sh

set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if ! git diff --cached --quiet; then
    echo "error: the index already contains staged changes" >&2
    echo "commit or unstage them before running this script" >&2
    exit 1
fi

commit_group() {
    local subject="$1"
    local body="$2"
    shift 2

    git add -- "$@"
    if git diff --cached --quiet; then
        echo "skip (no staged changes): $subject"
        return 0
    fi
    git commit -m "$subject" -m "$body"
}

commit_group \
    "build(workspace): unify cross-target build and dependency policy" \
    "Add the shared build foundation used by every crate, port and board:
workspace manifests and lockfile, cross-target cargo configuration, the root
target setup script, dependency deny rules, license and ignore policy. Host,
bare-metal, QEMU and Renode targets now resolve from one workspace." \
    .cargo/config.toml Cargo.toml Cargo.lock build.rs deny.toml LICENSE .gitignore

commit_group \
    "docs(release): publish implementation status and simulation results" \
    "Document the completed M1-M4 scope, the 22-board QEMU/Renode matrix,
GDB coverage and the remaining verification limits. Include the independent
audit report so the release claims can be traced back to reproduced results." \
    README.md RENOVER_SUMMARY.md SUMMARY.md reports

commit_group \
    "docs(plans): record the M1-M4 and RISC-V ecosystem completion plans" \
    "Capture the implementation plan for the industrial M2 milestone, the
complete RISC-V ecosystem, and the remaining integration work. These plans
describe the board, verification and documentation acceptance criteria used
for this release." \
    .cursor/plans

commit_group \
    "ci: add host, Kani, QEMU, Renode and GDB verification workflows" \
    "Add CI coverage for workspace tests, bounded Kani proofs, QEMU and Renode
board matrices, GDB smoke checks and pull-request review guidance. The
hardware-smoke workflow keeps simulator and host verification reproducible." \
    .github

commit_group \
    "feat(cap): implement derivation, attenuation and total revocation" \
    "Complete the capability authority model across CSpace allocation, handle
generation checks, parent links, epoch invalidation, derivation-tree
attenuation, safety/audit bits and recursive revocation. Add the builder and
set APIs, benchmarks and crate documentation used by the kernel." \
    crates/caplet-cap

commit_group \
    "feat(sync): add blocking mutex, semaphore, event and channel primitives" \
    "Wire synchronization waiters into the task waker and scheduler so threads
block instead of spinning. Implement mutex, semaphore, event, ticket lock,
channel and synchronous channel semantics, with wake-up paths covered by
tests." \
    crates/caplet-sync

commit_group \
    "feat(budget): enforce per-task CPU, memory and message quotas" \
    "Add budget accounting and a quota-aware channel that charges work before
admitting it. Exhaustion, blocking and wake-up behaviour are covered by tests
so resource limits are enforced by the kernel rather than advisory metadata." \
    crates/caplet-budget

commit_group \
    "feat(timer): add a scalable timer wheel and tick integration" \
    "Replace the fixed timer path with a wheel that supports relative and
periodic deadlines, deterministic expiration ordering and integration with
the scheduler tick and tickless-idle logic." \
    crates/caplet-timer

commit_group \
    "feat(async): complete the executor, waker and spawner runtime" \
    "Implement the async task lifecycle end to end: spawner registration,
executor polling, wake queue delivery, wake sources and a real waker. The
smoke tests exercise spawning, wake-ups, block_on and executor shutdown." \
    crates/caplet-async

commit_group \
    "feat(task): connect the task table and builder to kernel scheduling" \
    "Add the shared task table, task references and state transitions used by
the scheduler. The builder now carries priority, stack and argument metadata
into the kernel task runtime instead of maintaining a separate TCB model." \
    crates/caplet-task

commit_group \
    "feat(macros): generate task metadata and expose the unified facade" \
    "Generate kernel-visible metadata from #[task] and #[main], including
priority, stack and entry wiring. Add the exported macro layer and the
caplet facade crate with smoke tests for the public API." \
    crates/caplet-macros crates/caplet-macros-exports crates/caplet

commit_group \
    "feat(kernel): initialize processes, task ownership and per-process CSpaces" \
    "Bring up the kernel from boot to the first task with per-process CSpaces,
task-owner lookup, a boot-task CSpace fast path, process/resource tables and
architecture entry points. Wire the main task, kernel configuration and
initialization sequence for RAM and MMU-capable targets." \
    crates/caplet-kernel/Cargo.toml crates/caplet-kernel/README.md \
    crates/caplet-kernel/benches/context_switch_bench.rs \
    crates/caplet-kernel/src/arch.rs crates/caplet-kernel/src/boot.rs \
    crates/caplet-kernel/src/cpu.rs crates/caplet-kernel/src/kernel.rs \
    crates/caplet-kernel/src/kernel_config.rs crates/caplet-kernel/src/kernel_init.rs \
    crates/caplet-kernel/src/lib.rs crates/caplet-kernel/src/main_task.rs \
    crates/caplet-kernel/src/process.rs crates/caplet-kernel/src/resource_table.rs

commit_group \
    "feat(kernel): add preemptive priority scheduling, tickless idle and SMP" \
    "Implement priority-ordered ready queues, round-robin time slices, tickless
idle and WFI handling. Add per-CPU scheduling, affinity-aware migration and
secondary-core bring-up so the same scheduler runs on single-core and SMP
targets." \
    crates/caplet-kernel/src/scheduler.rs crates/caplet-kernel/src/ready_queue.rs \
    crates/caplet-kernel/src/priority.rs crates/caplet-kernel/src/smp.rs \
    crates/caplet-kernel/src/smp_scheduler.rs crates/caplet-kernel/src/tickless.rs

commit_group \
    "feat(kernel): add MMU-backed memory isolation and allocator integration" \
    "Connect page-table construction, permission checks, user/kernel address
spaces and the bump/slot allocators. Memory grants now flow through the
kernel resource layer and support protected mappings on MMU-capable ports." \
    crates/caplet-kernel/src/memory.rs crates/caplet-kernel/src/mmu.rs \
    crates/caplet-kernel/src/bump_allocator.rs crates/caplet-kernel/src/slot_allocator.rs

commit_group \
    "feat(kernel): complete capability, memory, task and timer syscalls" \
    "Implement the system-call boundary for capability derivation, duplication,
grant and revocation; memory, task and timer operations; and M3 compatibility
traps. Out-of-range handles and invalid buffers are rejected before dispatch,
with results returned through a uniform encoding." \
    crates/caplet-kernel/src/syscall.rs crates/caplet-kernel/src/syscall_cap.rs \
    crates/caplet-kernel/src/syscall_dispatch.rs crates/caplet-kernel/src/syscall_m3.rs \
    crates/caplet-kernel/src/syscall_memory.rs crates/caplet-kernel/src/syscall_task.rs \
    crates/caplet-kernel/src/syscall_timer.rs

commit_group \
    "feat(kernel): add module, quota, security-grant and namespace IPC services" \
    "Add module loading metadata, quota enforcement, security-grant checking and
namespace-scoped IPC. Connect crypto and flash syscalls to the grant model so
privileged operations require an explicit capability." \
    crates/caplet-kernel/src/ipc.rs crates/caplet-kernel/src/ipc_namespace.rs \
    crates/caplet-kernel/src/module.rs crates/caplet-kernel/src/quota.rs \
    crates/caplet-kernel/src/security_grants.rs crates/caplet-kernel/src/syscall_crypto.rs \
    crates/caplet-kernel/src/syscall_flash.rs crates/caplet-kernel/src/syscall_ipc.rs

commit_group \
    "test(kernel): cover capability grants, module security and task sleep" \
    "Add host tests for capability grants, namespace isolation, module and
security syscalls, task sleep/wake behaviour and the existing syscall/SMP
smoke paths. These tests exercise denial and boundary cases, not only the
successful path." \
    crates/caplet-kernel/tests

commit_group \
    "feat(hal): define portable GPIO, UART, SPI, I2C, timer and IRQ contracts" \
    "Unify the hardware abstraction contracts used by all boards, including
GPIO pin configuration, UART transactions, SPI/I2C transfer semantics, timer
deadlines, interrupt registration, LED/PHY helpers and the HAL version gate.
Add an API contract test so backends stay interchangeable." \
    crates/caplet-hal

commit_group \
    "feat(stm32f4): implement flash, RCC, GPIO, serial, SPI/I2C and PHY drivers" \
    "Add the STM32F4 register layer and drivers for flash, clock control, GPIO,
UART, SPI, I2C, timers, interrupts and the DP83848 Ethernet PHY. The drivers
share the common HAL traits and support the QEMU and Renode STM32F4 boards." \
    crates/caplet-hal-stm32f4

commit_group \
    "feat(nrf52): add GPIO, UART, RTC, timer, SPIM, TWIM and IRQ drivers" \
    "Implement the nRF52 peripheral layer with GPIO, UART, RTC, timer, SPI
master, TWI master and interrupt support. Register and raw-access helpers keep
the backend self-contained for bare-metal builds." \
    crates/caplet-hal-nrf52

commit_group \
    "feat(gd32): add the GD32 platform HAL baseline" \
    "Introduce the GD32 HAL crate and its common trait implementations so the
platform can be selected alongside STM32F4 and nRF52 without changing board
code." \
    crates/caplet-hal-gd32

commit_group \
    "feat(log): add UART, semihosting and structured formatting backends" \
    "Add levelled logging with writer selection, UART and semihosting output,
and shared formatting helpers. The crate can be compiled for host tests and
bare-metal targets without changing call sites." \
    crates/caplet-log

commit_group \
    "feat(sensors): add the sensor abstraction and benchmark harness" \
    "Provide common sensor reading/error traits used by the industrial demos,
plus a benchmark harness that records kernel and I/O measurements in a
consistent format." \
    crates/caplet-sensors crates/caplet-bench-harness

commit_group \
    "feat(fs): implement VFS, FAT32, LittleFS and power-cut-safe persistence" \
    "Implement capability-mediated VFS handles, RAMFS, FAT32 and LittleFS with
mounting, path lookup, file I/O and boot images. Add power-cut detection and
recovery, a larger FAT32 fixture, mkfs coverage and filesystem documentation." \
    crates/caplet-fs

commit_group \
    "feat(net): complete IPv4, ARP, DHCP, DNS, ICMP and TCP networking" \
    "Implement Ethernet framing, ARP, IPv4, DHCP, DNS, ICMP, the TCP state
machine and the socket table. Capability-mediated network syscalls and
benchmarks expose the stack to tasks without ambient network authority." \
    crates/caplet-net

commit_group \
    "feat(mqttsn): add a resilient MQTT-SN client and wire-level E2E coverage" \
    "Implement MQTT-SN connection, publish, subscribe and acknowledgment flows
with retry/backoff and reconnect handling. The in-process broker test parses
real wire bytes and verifies PUBACK/recovery behaviour." \
    crates/caplet-mqttsn

commit_group \
    "feat(loader): implement ELF/.capp loading, relocation and sandbox checks" \
    "Add ELF and CAPP parsing, allocation and relocation, manifest capability
validation, signature-aware verification and sandbox policy enforcement. Add
loader metrics, throughput benchmarks and host integration tests." \
    crates/caplet-loader

commit_group \
    "feat(security): add crypto, flash, JTAG and token protection primitives" \
    "Add the shared crypto implementation and the kernel-facing security
services for flash locking, JTAG disablement and device tokens. These
primitives back secure boot, OTA verification and the security-grant syscalls." \
    crates/caplet-crypto crates/caplet-security

commit_group \
    "feat(ota): implement A/B OTA, verification and rollback" \
    "Implement image download, flash programming, metadata verification,
active-slot switching and automatic rollback. Add boot manager, rollback and
verifier tests covering corruption and interrupted-update scenarios." \
    crates/caplet-ota

commit_group \
    "feat(riscv): add RV32/RV64 boot, MMU/PMP, CLINT, PLIC and context switching" \
    "Implement RISC-V supervisor-mode boot, trap entry, context switching,
page-table setup, CLINT timers, PLIC interrupts and UART logging. Linker and
memory layouts support both rv32imac and rv64gc QEMU/Renode targets." \
    ports/riscv

commit_group \
    "feat(aarch64): bring up EL1/EL0, GICv3, MMU, exceptions and UART" \
    "Add the AArch64 boot path, exception vectors, context switching, GICv3
interrupt routing, translation-table setup, tick handling and PL011 UART
output. The port targets the QEMU virt and Renode Cortex-A53 models." \
    ports/aarch64

commit_group \
    "feat(cortex-a): add ARMv7-A MMU, GIC, SMC, SMP and PL011 support" \
    "Implement ARMv7-A boot, section-table MMU mappings, GIC interrupt handling,
SMC calls, PL011 console, timer ticks and secondary-core startup. Shared page
tables and affinity-aware scheduling support Cortex-A9 and Allwinner A20." \
    ports/cortex-a

commit_group \
    "feat(cortex-m): add NVIC, PendSV, SysTick, tickless idle and UART support" \
    "Implement Cortex-M boot and exception entry, PendSV context switching,
NVIC registration, SysTick timing, tickless idle and QEMU UART output. Panic
and kernel-bridge paths are shared by Cortex-M3/M4 boards." \
    ports/cortex-m

commit_group \
    "feat(allwinner): add Allwinner A20 UART and platform bring-up" \
    "Add the Allwinner A20 platform crate and UART backend used by the QEMU
cubieboard and Renode A20 targets, including the minimal boot-facing hooks
needed by the shared Cortex-A port." \
    ports/allwinner

commit_group \
    "feat(board): bring up the Cortex-M3 QEMU/Renode image" \
    "Add the MPS2-AN385 board package with linker layout, kernel features,
startup entry and a Renode platform description. The image verifies boot,
scheduler and capability startup with the shared completion marker." \
    boards/hello-qemu-cm3

commit_group \
    "feat(board): bring up the Cortex-M4 QEMU/Renode image" \
    "Add the MPS2 Cortex-M4 board package, SMSC91C111 Ethernet device and Renode
platform models. The image exercises scheduling, GPIO/network initialization
and the shared console path on QEMU and Renode." \
    boards/hello-qemu-cm4

commit_group \
    "feat(board): add the Cortex-M4 filesystem demo" \
    "Add a Cortex-M4 image that mounts the VFS, performs file operations and
reports completion. Its Renode model shares the MPS2 memory map so the same
ELF verifies storage behaviour in both simulators." \
    boards/hello-qemu-cm4-fs

commit_group \
    "feat(board): add Nucleo-F446ZE QEMU and hardware-target packages" \
    "Add the STM32F446ZE board packages, linker/memory layouts, critical-section
integration, UART startup and Renode platform descriptions for the QEMU
compatibility target and the physical-board target." \
    boards/hello-nucleo-f446ze-qemu boards/nucleo-f446ze

commit_group \
    "feat(board): add the RISC-V 32/64 bare-metal image" \
    "Add the RISC-V board package with rv32/rv64 feature selection, linker
layouts, OpenSBI-compatible boot entry and QEMU/Renode virt models. The image
verifies trap, timer, capability and console initialization." \
    boards/hello-riscv

commit_group \
    "feat(board): add the AArch64 QEMU/Renode image" \
    "Add the AArch64 board package with EL1 startup, GICv3 and MMU features,
linker/memory layout and Renode Cortex-A53 platform descriptions. It verifies
exception entry, timer ticks and early task startup." \
    boards/hello-aarch64-qemu

commit_group \
    "feat(board): add the Cortex-A9 QEMU/Renode image" \
    "Add the ARMv7-A Cortex-A9 board package with MMU section tables, GIC,
timer and UART setup. QEMU vexpress-a9 and the Renode Cortex-A9 model use the
same ELF and completion assertion." \
    boards/hello-cortex-a9

commit_group \
    "feat(board): add the Allwinner A20 QEMU/Renode image" \
    "Add the A20 board package with cubieboard memory layout, UART startup and
Renode A20 platform model. The image verifies the shared Cortex-A boot path on
both simulators." \
    boards/hello-allwinner-a20-qemu

commit_group \
    "feat(demo): deliver the industrial sensor-to-MQTT-SN scenario" \
    "Implement the Cortex-M4 industrial demo: sensor sampling, capability-scoped
storage, budgeted tasks, MQTT-SN publish and watchdog reporting. Add Renode
models for Cortex-M4, RV32 and RV64 variants." \
    boards/industrial-demo

commit_group \
    "feat(demo): deliver the energy-meter industrial scenario" \
    "Implement the energy-meter application with metering tasks, quota
accounting, persistent records and MQTT-SN telemetry. Cortex-M4 and RISC-V
board variants share the same scenario logic and verification markers." \
    boards/energy-meter

commit_group \
    "feat(demo): deliver the medical-device safety scenario" \
    "Implement the medical-device demo with isolated sensor/control tasks,
capability-scoped access and alarm handling. The board package is verified on
the Cortex-M4, RV32 and RV64 simulator variants." \
    boards/medical-device

commit_group \
    "feat(demo): deliver the automotive CAN control scenario" \
    "Implement the automotive demo with periodic control, watchdog and
capability-separated sensor/actuator paths. Cortex-M4 and RISC-V variants share
the scenario state machine and Renode assertions." \
    boards/automotive

commit_group \
    "feat(demo): add the host-side dynamic loader example" \
    "Add an application that loads and executes an ELF/.capp module through the
loader API, validates its manifest capabilities and reports the sandboxed
entry result. This is a host end-to-end demonstration of dynamic loading." \
    boards/hello-dynamic-loader

commit_group \
    "test(sim): add the QEMU architecture matrix" \
    "Add QEMU runners for RISC-V32/64, AArch64, Cortex-A9 and Allwinner A20.
Each runner uses the shared board specification, asserts the functional
completion marker and fails on boot or console errors." \
    tests/qemu-riscv32 tests/qemu-riscv64 tests/qemu-aarch64-qemu \
    tests/qemu-cortex-a9 tests/qemu-hello-allwinner-a20-qemu

commit_group \
    "test(sim): add Cortex-M and STM32F4 QEMU coverage" \
    "Add QEMU runners for Cortex-M3, Cortex-M4, the filesystem image, the
Nucleo-F446ZE compatibility target and the STM32F4 board. The runners verify
boot, storage, networking initialization and console output." \
    tests/qemu-cm3 tests/qemu-cm4 tests/qemu-cm4-fs tests/qemu-stm32f4 \
    tests/qemu-hello-nucleo-f446ze-qemu tests/qemu-nucleo-f446ze

commit_group \
    "test(sim): add tickless and SMP QEMU smoke tests" \
    "Add dedicated QEMU tests that verify tickless idle entry/exit and
multi-core scheduling progress. The tests assert progress from every expected
CPU/task rather than accepting a single console line." \
    tests/qemu-cm4-tickless tests/qemu-smp-smoke

commit_group \
    "test(renode): add the Renode architecture matrix" \
    "Add Renode runners for RISC-V32/64, AArch64, Cortex-A9 and Allwinner A20.
The runners reject monitor errors and unhandled accesses, capture logs and
assert the same functional markers as QEMU." \
    tests/renode-riscv32 tests/renode-riscv64 tests/renode-aarch64-qemu \
    tests/renode-cortex-a9 tests/renode-hello-allwinner-a20-qemu

commit_group \
    "test(renode): add Cortex-M, filesystem and Nucleo coverage" \
    "Add Renode runners for Cortex-M3/M4, the filesystem image, the Nucleo
platform and the STM32F4 family. Each run validates boot, task progress and
storage or peripheral behaviour against the shared expected marker." \
    tests/renode-cm3 tests/renode-cm4 tests/renode-cm4-fs tests/renode-nucleo

commit_group \
    "test(e2e): cover industrial, tickless and dynamic-loading scenarios" \
    "Add end-to-end fixtures for the industrial demos, tickless scheduler
behaviour and host-side dynamic loading. The scripts build the target, run the
scenario and validate the emitted application records or loader result." \
    tests/industrial-demo tests/industrial-e2e tests/dynamic-loader-e2e

commit_group \
    "feat(sim): add shared QEMU/Renode/GDB board orchestration" \
    "Add one board specification shared by QEMU, Renode and GDB, with 22 board
keys, target triples, platform files and expected markers. Add serial matrix
runners and early-breakpoint GDB support for RISC-V64, AArch64 and ARMv7-A." \
    tools/sim run.sh

commit_group \
    "test(tools): add HAL drift, tickless and grep verification gates" \
    "Add automated checks for HAL API drift, tickless invariants and prohibited
code patterns. The allowlist keeps intentional exceptions reviewable while CI
fails on new violations." \
    tools/grep_gate.sh tools/grep_gate.allowlist tools/hal_version_gate.sh \
    tools/tickless_verify.sh tools/tickless_verify

commit_group \
    "test(kani): add the bounded verification catalogue and harnesses" \
    "Add Kani proofs for capability derivation, attenuation and revocation,
syscall dispatch, MMU permissions, loader manifests, DHCP/socket state, SMP
IPC and MQTT-SN state. Include per-harness runners, aggregation scripts and
reuse guidance." \
    tools/kani

commit_group \
    "feat(tools): add CAPP conversion and migration analysis utilities" \
    "Add the ELF-to-CAPP conversion helper and the HAL migration and stack
analysis tool crates. The migration and stack tools are explicitly scoped as
M3 analysis utilities rather than completed production analyzers." \
    tools/elf2capp.py tools/hal-migrate tools/stack-analyzer

commit_group \
    "docs: add the simulation, porting, Kani and security guides" \
    "Document the shared board matrix, Renode/QEMU debugging workflow, porting
steps, Kani proof catalogue, industrial demo and capability security model.
Update the quick-start and existing architecture guides to match the
implemented interfaces." \
    docs/dev-quickstart.md docs/industrial-demo.md docs/kani-guide.md \
    docs/kani.md docs/porting-guide.md docs/porting.md \
    docs/security-model.md docs/simulation.md

commit_group \
    "docs(adr): record capability, async, tickless, VFS, loader and SMP decisions" \
    "Add the accepted ADRs for capability authority, async concurrency, tickless
idle, resource budgets, VFS handles, the self-written network stack, the
industrial MQTT-SN scenario, linkme fallback, TCP scope, loader formats,
Cortex-A9 MMU, SMP and loader observability." \
    docs/adr

commit_group \
    "feat(examples): add the Caplet UI demo workspace" \
    "Add a standalone example workspace and UI demo that consumes the public
CapletRT facade, demonstrating task spawning and capability-scoped services
without depending on an internal board package." \
    examples

git add -- push.sh
git commit -m "chore(release): add the reviewed multi-commit release script" \
    -m "Add the explicit release script that groups the completed work into
feature-level commits, validates that no tracked changes are left behind and
pushes only after all commits succeed."

leftovers="$(git status --porcelain)"
if [ -n "$leftovers" ]; then
    echo "error: uncommitted changes remain; push aborted" >&2
    printf '%s\n' "$leftovers" >&2
    exit 1
fi

branch="$(git branch --show-current)"
if [ -z "$branch" ]; then
    echo "error: detached HEAD; refusing to push" >&2
    exit 1
fi

if [ "${NO_PUSH:-0}" = "1" ]; then
    echo "NO_PUSH=1: commits created on '$branch'; push skipped"
else
    git push -u origin "$branch"
fi
