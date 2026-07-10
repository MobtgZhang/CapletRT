#!/bin/bash
# Push CapletRT to GitHub
# This script stages all files except docv1.md, commits, and pushes to origin main

set -e

cd /home/mobtgzhang/github2/CapletRT

# Stage all files except docv1.md
git add .

# Unstage docv1.md if it was accidentally staged
git reset HEAD docv1.md 2>/dev/null || true

# Create commit with comprehensive message
git commit -m "$(cat <<'EOF'
feat: initial CapletRT embedded RTOS framework

Add complete capability-based embedded RTOS framework with:

Kernel & Scheduler:
- SMP scheduler with priority-based ready queue
- Context switch support for ARM Cortex-M
- Bump allocator for dynamic memory
- Slot and capability cspace management

Task & IPC:
- Task management with task builder pattern
- IPC namespace for inter-process communication
- Sync primitives: mutex, semaphore, channel, event, ticket lock

Peripheral HAL:
- HAL abstractions for GPIO, UART, SPI, I2C, Timer, interrupts
- Platform-specific HALs: STM32F4, NRF52, GD32

Filesystem & Networking:
- VFS with FAT32 and LittleFS support
- RAMFS and boot image mounting
- TCP/IP stack with DHCP, DNS, ICMP support

OTA & Security:
- OTA update with A/B partition and rollback
- Crypto: AES-128/256, SHA256, HMAC, RSA
- Flash protection and secure boot

Loader & Capability:
- ELF dynamic loader with sandboxed execution
- Capability system with attenuation and revocation
- Resource budgets and rights management

Boards & Demos:
- QEMU Cortex-M3/M4F targets
- RISC-V target
- Industrial MQTT-SN demo
- Medical device and energy meter examples
- Automotive board support

Async Runtime:
- Executor with wake queue
- Spawner and waker implementation
- Budget-based async task scheduling

Docs & CI:
- Architecture decision records (ADRs)
- GitHub Actions CI pipeline
- Comprehensive test coverage

Closes initial framework delivery for M1/M2 industrial use cases.
EOF
)"

# Push to remote
git push origin main
