//! SPI trait definitions

/// SPI mode (clock polarity and phase)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpiMode {
    /// Clock polarity (0 = low when idle, 1 = high when idle)
    pub polarity: bool,
    /// Clock phase (0 = sample on first edge, 1 = sample on second edge)
    pub phase: bool,
}

impl SpiMode {
    /// Mode 0: CPOL=0, CPHA=0
    pub fn mode0() -> Self {
        Self {
            polarity: false,
            phase: false,
        }
    }

    /// Mode 1: CPOL=0, CPHA=1
    pub fn mode1() -> Self {
        Self {
            polarity: false,
            phase: true,
        }
    }

    /// Mode 2: CPOL=1, CPHA=0
    pub fn mode2() -> Self {
        Self {
            polarity: true,
            phase: false,
        }
    }

    /// Mode 3: CPOL=1, CPHA=1
    pub fn mode3() -> Self {
        Self {
            polarity: true,
            phase: true,
        }
    }
}

impl Default for SpiMode {
    fn default() -> Self {
        Self::mode0()
    }
}

/// SPI data size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSize {
    /// 8-bit data
    Bits8,
    /// 16-bit data
    Bits16,
}

/// SPI bit order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOrder {
    /// Most significant bit first
    MsbFirst,
    /// Least significant bit first
    LsbFirst,
}

/// SPI configuration
#[derive(Debug, Clone, Copy)]
pub struct SpiConfig {
    /// SPI mode
    pub mode: SpiMode,
    /// Clock frequency in Hz
    pub frequency: u32,
    /// Data size
    pub data_size: DataSize,
    /// Bit order
    pub bit_order: BitOrder,
}

impl Default for SpiConfig {
    fn default() -> Self {
        Self {
            mode: SpiMode::default(),
            frequency: 1_000_000,
            data_size: DataSize::Bits8,
            bit_order: BitOrder::MsbFirst,
        }
    }
}

/// SPI trait for serial communication
pub trait Spi {
    /// Error type
    type Error;

    /// Configure the SPI bus
    fn configure(&mut self, config: SpiConfig) -> Result<(), Self::Error>;

    /// Transfer data (full duplex)
    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error>;

    /// Write data
    fn write(&mut self, data: &[u8]) -> Result<(), Self::Error>;

    /// Read data
    fn read(&mut self, buffer: &mut [u8]) -> Result<(), Self::Error>;
}
