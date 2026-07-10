# `caplet-log`

`no_std` logging framework with pluggable backends.

## Backends

| Backend          | When to use                                       |
|------------------|---------------------------------------------------|
| `semihosting`    | QEMU / debug probe.                               |
| `uart`           | Real hardware: pipe messages to a UART.           |

Backends implement the `LogWriter` trait. Install them with
`install_writer(Some(&writer))`.

## Usage

```rust
use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};

install_writer(Some(&SEMIHOSTING_INSTANCE));
info!("boot", "hello from task {}", name);
```

## Status

Stable. Used by all demo boards.