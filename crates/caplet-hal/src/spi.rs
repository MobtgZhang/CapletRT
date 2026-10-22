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

/// Explicit SPI mode number.
///
/// [`SpiMode`] carries the two electrical bits because that is what the
/// hardware programs. This enum gives board code a single, exhaustive way to
/// select one of the four standard modes without remembering the CPOL/CPHA
/// combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiModeId {
    /// CPOL=0, CPHA=0
    Mode0,
    /// CPOL=0, CPHA=1
    Mode1,
    /// CPOL=1, CPHA=0
    Mode2,
    /// CPOL=1, CPHA=1
    Mode3,
}

impl From<SpiModeId> for SpiMode {
    fn from(mode: SpiModeId) -> Self {
        match mode {
            SpiModeId::Mode0 => Self::mode0(),
            SpiModeId::Mode1 => Self::mode1(),
            SpiModeId::Mode2 => Self::mode2(),
            SpiModeId::Mode3 => Self::mode3(),
        }
    }
}

impl SpiMode {
    /// Return the standard mode number, if the polarity/phase pair is one of
    /// the four valid combinations.
    pub fn id(self) -> Option<SpiModeId> {
        match (self.polarity, self.phase) {
            (false, false) => Some(SpiModeId::Mode0),
            (false, true) => Some(SpiModeId::Mode1),
            (true, false) => Some(SpiModeId::Mode2),
            (true, true) => Some(SpiModeId::Mode3),
        }
    }
}

/// Chip-select helpers.
///
/// `ActiveLow` is a zero-cost wrapper around a board-specific chip-select
/// pin. It deliberately does not own the SPI bus: selecting a device and
/// transferring bytes remain separate operations so a caller cannot leave CS
/// asserted while another task uses the bus.
pub mod cs {
    /// A chip-select line whose asserted level is logic low.
    #[derive(Debug)]
    pub struct ActiveLow<C> {
        inner: C,
    }

    impl<C> ActiveLow<C> {
        /// Wrap a chip-select pin.
        pub const fn new(inner: C) -> Self {
            Self { inner }
        }

        /// Access the underlying pin.
        pub fn inner(&self) -> &C {
            &self.inner
        }

        /// Mutably access the underlying pin.
        pub fn inner_mut(&mut self) -> &mut C {
            &mut self.inner
        }

        /// Consume the wrapper and return the pin.
        pub fn release(self) -> C {
            self.inner
        }
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
