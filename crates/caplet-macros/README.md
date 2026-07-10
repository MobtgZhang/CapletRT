# `caplet-macros`

Procedural macros for defining tasks and the kernel entry
point.

## Macros

| Macro                | Expands to                                   |
|----------------------|----------------------------------------------|
| `#[caplet::task]`    | Spawn a future into the executor with config.|
| `#[caplet::main]`    | Define the kernel entry function.            |

## Example

```rust
#[caplet_task(priority = 3, stack = 4096, name = "logger")]
async fn logger_task() {
    info!("logger", "alive");
}

#[caplet::main]
fn main() -> ! {
    spawn(logger_task());
    drive_executor_forever()
}
```

## Status

Stable.