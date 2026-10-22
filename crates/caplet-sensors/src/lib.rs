//! Sensor abstraction and implementations.
//!
//! `Sensor` is the generic trait used by the industrial demo
//! to read temperature / pressure / whatever. The board
//! instantiates one or more concrete implementations
//! ([`Tmp102`], [`MockSensor`]).

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

/// Reading from a sensor. The meaning of `value` is unit-
/// dependent — see each sensor's documentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// Stable device identifier (NUL-padded ASCII, at most 16 bytes).
    pub device_id: [u8; 16],
    /// Sensor value (interpreted by the concrete type).
    pub value: f32,
    /// Timestamp in milliseconds since boot.
    pub timestamp_ms: u64,
}

/// Trait every sensor implements.
pub trait Sensor {
    /// Human-readable name (e.g. `"tmp102"`).
    fn name(&self) -> &str;

    /// Read the current value.
    fn read(&self) -> Reading;

    /// Whether the sensor reports a valid reading right now.
    fn is_ready(&self) -> bool;
}

/// A mock sensor with a user-defined temperature curve. Used by
/// the industrial demo for the QEMU smoke test.
#[derive(Debug, Clone)]
pub struct MockSensor {
    /// Sensor name.
    pub name: alloc::string::String,
    /// Name supplied through the allocation-free constructor.
    static_name: Option<&'static str>,
    /// Current value (mutated by the test harness).
    pub value: core::cell::Cell<f32>,
}

impl MockSensor {
    /// Construct a mock with `initial` value.
    pub fn new(name: &str, initial: f32) -> Self {
        Self {
            name: alloc::string::String::from(name),
            static_name: None,
            value: core::cell::Cell::new(initial),
        }
    }

    /// `const`-fn constructor that avoids the allocation
    /// needed by [`Self::new`]. Useful for `static` items
    /// where `String::from` is not available.
    pub const fn new_init(name: &'static str, initial: f32) -> Self {
        Self {
            name: alloc::string::String::new(),
            static_name: Some(name),
            value: core::cell::Cell::new(initial),
        }
    }

    /// Update the value the sensor will report.
    pub fn set_value(&self, v: f32) {
        self.value.set(v);
    }
}

impl Sensor for MockSensor {
    fn name(&self) -> &str {
        self.static_name.unwrap_or(&self.name)
    }
    fn read(&self) -> Reading {
        let mut device_id = [0u8; 16];
        let bytes = self.name().as_bytes();
        let len = bytes.len().min(device_id.len());
        device_id[..len].copy_from_slice(&bytes[..len]);
        Reading {
            device_id,
            value: self.value.get(),
            timestamp_ms: 0,
        }
    }
    fn is_ready(&self) -> bool {
        true
    }
}

/// TMP102 temperature sensor.
///
/// In the real implementation this would drive an I2C bus
/// through the [`caplet_hal::i2c::I2c`] trait. For the
/// scaffold we hold a static value and provide the wire-format
/// encode/decode that the I2C driver uses.
#[derive(Debug, Clone)]
pub struct Tmp102 {
    /// Latest temperature in degrees Celsius.
    pub temperature: core::cell::Cell<f32>,
}

impl Default for Tmp102 {
    fn default() -> Self {
        Self::new()
    }
}

impl Tmp102 {
    /// 7-bit I2C address.
    pub const ADDR: u8 = 0x48;
    /// Temperature register.
    pub const REG_TEMP: u8 = 0x00;
    /// Config register.
    pub const REG_CONFIG: u8 = 0x01;

    /// Construct a TMP102 with 25.0 °C as the initial reading.
    pub fn new() -> Self {
        Self {
            temperature: core::cell::Cell::new(25.0),
        }
    }

    /// Update the cached temperature (called by the I2C poll
    /// task).
    pub fn set_temperature(&self, t: f32) {
        self.temperature.set(t);
    }

    /// Encode a temperature reading into the TMP102's 12-bit
    /// two's-complement format. The 4 LSBs are zero; the
    /// effective resolution is 0.0625 °C.
    pub fn encode_temperature(t: f32) -> u16 {
        // Clamp to the supported range.
        let t_clamped = t.clamp(-55.0, 127.0);
        let scaled = (t_clamped / 0.0625) as i16;
        ((scaled as u16) << 4) & 0xFFF0
    }

    /// Decode the TMP102's 16-bit register value into °C.
    /// Only the upper 12 bits are interpreted.
    pub fn decode_temperature(raw: u16) -> f32 {
        let upper = (raw >> 4) & 0x0FFF;
        // Sign-extend a 12-bit value to 16 bits.
        let signed = if upper & 0x0800 != 0 {
            (upper | 0xF000) as i16
        } else {
            upper as i16
        };
        (signed as f32) * 0.0625
    }
}

impl Sensor for Tmp102 {
    fn name(&self) -> &str {
        "tmp102"
    }
    fn read(&self) -> Reading {
        let mut device_id = [0u8; 16];
        device_id[..6].copy_from_slice(b"tmp102");
        Reading {
            device_id,
            value: self.temperature.get(),
            timestamp_ms: 0,
        }
    }
    fn is_ready(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_sensor_basic() {
        let s = MockSensor::new("mock", 23.5);
        assert_eq!(s.name(), "mock");
        assert_eq!(&s.read().device_id[..4], b"mock");
        assert_eq!(s.read().value, 23.5);
        s.set_value(24.0);
        assert_eq!(s.read().value, 24.0);
    }

    #[test]
    fn mock_sensor_const_constructor_keeps_name() {
        let sensor = MockSensor::new_init("mock-temp", 22.0);
        assert_eq!(sensor.name(), "mock-temp");
        assert_eq!(&sensor.read().device_id[..9], b"mock-temp");
    }

    #[test]
    fn tmp102_encode_decode_roundtrip() {
        let raw = Tmp102::encode_temperature(23.5);
        let decoded = Tmp102::decode_temperature(raw);
        assert!((decoded - 23.5).abs() < 0.1);
    }

    #[test]
    fn tmp102_negative_temperature() {
        let raw = Tmp102::encode_temperature(-10.0);
        let decoded = Tmp102::decode_temperature(raw);
        assert!((decoded - (-10.0)).abs() < 0.1);
    }
}
