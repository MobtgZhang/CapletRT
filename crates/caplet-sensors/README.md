# `caplet-sensors`

Sensor abstraction with two implementations used by the
industrial demo.

## Implementations

| Type         | Description                                      |
|--------------|--------------------------------------------------|
| `MockSensor` | Deterministic, `const fn`-constructible.         |
| `Tmp102`     | Texas Instruments TMP102 temperature sensor.     |

## Trait

```rust
pub trait Sensor {
    type Reading;
    fn read(&self) -> Self::Reading;
}
```

## Hardware

`Tmp102` is implemented against `caplet-hal::I2c`; to use it
on real hardware, instantiate with a platform I2C peripheral
behind a `SpinLock`. The industrial demo uses `MockSensor`.