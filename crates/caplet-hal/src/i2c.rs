//! I2C trait definitions

/// I2C address mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2cAddrMode {
    /// 7-bit address
    Addr7Bit,
    /// 10-bit address
    Addr10Bit,
}

/// I2C speed mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2cSpeed {
    /// Standard mode (100 kHz)
    Standard,
    /// Fast mode (400 kHz)
    Fast,
    /// Fast mode plus (1 MHz)
    FastPlus,
    /// High speed mode (3.4 MHz)
    HighSpeed,
}

/// I2C configuration
#[derive(Debug, Clone, Copy)]
pub struct I2cConfig {
    /// I2C address
    pub address: u16,
    /// Address mode
    pub addr_mode: I2cAddrMode,
    /// Speed mode
    pub speed: I2cSpeed,
    /// Timeout in milliseconds
    pub timeout_ms: u32,
}

/// I2C transfer error classification.
///
/// Drivers that expose more detail than the generic HAL error can return
/// these values through [`HalError`](crate::HalError), which already provides
/// the matching `BusBusy`, `ArbitrationLost`, and `Nack` variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2cError {
    /// Another master currently owns the bus.
    BusBusy,
    /// Two masters started a transfer at the same time and this one lost.
    ArbitrationLost,
    /// The addressed target did not acknowledge a byte.
    Nack,
    /// The transfer did not complete within its configured timeout.
    Timeout,
}

impl Default for I2cConfig {
    fn default() -> Self {
        Self {
            address: 0,
            addr_mode: I2cAddrMode::Addr7Bit,
            speed: I2cSpeed::Standard,
            timeout_ms: 100,
        }
    }
}

/// I2C trait for serial communication
pub trait I2c {
    /// Error type
    type Error;

    /// Configure the I2C bus
    fn configure(&mut self, config: I2cConfig) -> Result<(), Self::Error>;

    /// Write data to a device
    fn write(&mut self, address: u16, data: &[u8]) -> Result<(), Self::Error>;

    /// Read data from a device
    fn read(&mut self, address: u16, buffer: &mut [u8]) -> Result<(), Self::Error>;

    /// Write and then read data
    fn write_read(
        &mut self,
        address: u16,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error>;

    /// Scan for devices on the bus
    fn scan(&self) -> Result<alloc::vec::Vec<u8>, Self::Error>;

    /// Write using a DMA-capable backend when available.
    ///
    /// The default is a blocking transfer so every existing driver remains
    /// correct. DMA-capable drivers should override it and only report
    /// success after the completion interrupt has been observed.
    fn write_dma(&mut self, address: u16, data: &[u8]) -> Result<(), Self::Error> {
        self.write(address, data)
    }

    /// Read using a DMA-capable backend when available.
    fn read_dma(&mut self, address: u16, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.read(address, buffer)
    }

    /// Combined write/read transfer using the DMA path when available.
    fn write_read_dma(
        &mut self,
        address: u16,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.write_read(address, write, read)
    }
}
