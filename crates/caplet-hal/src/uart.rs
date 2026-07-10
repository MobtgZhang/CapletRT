//! UART trait definition

/// UART configuration
#[derive(Debug, Clone, Copy)]
pub struct UartConfig {
    /// Baud rate
    pub baud_rate: u32,
    /// Data bits (5-8)
    pub data_bits: u8,
    /// Stop bits
    pub stop_bits: StopBits,
    /// Parity
    pub parity: Parity,
}

impl UartConfig {
    /// Create default config at 115200 baud
    pub fn default_115200() -> Self {
        Self {
            baud_rate: 115200,
            data_bits: 8,
            stop_bits: StopBits::One,
            parity: Parity::None,
        }
    }

    /// Create config at specified baud rate
    pub fn new(baud_rate: u32) -> Self {
        Self {
            baud_rate,
            data_bits: 8,
            stop_bits: StopBits::One,
            parity: Parity::None,
        }
    }

    /// Set data bits
    pub fn with_data_bits(self, bits: u8) -> Self {
        Self {
            data_bits: bits.min(8).max(5),
            ..self
        }
    }

    /// Set stop bits
    pub fn with_stop_bits(self, stop: StopBits) -> Self {
        Self { stop_bits: stop, ..self }
    }

    /// Set parity
    pub fn with_parity(self, parity: Parity) -> Self {
        Self { parity, ..self }
    }
}

impl Default for UartConfig {
    fn default() -> Self {
        Self::default_115200()
    }
}

/// Stop bits configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBits {
    /// One stop bit
    One,
    /// Two stop bits
    Two,
}

/// Parity configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    /// No parity
    None,
    /// Even parity
    Even,
    /// Odd parity
    Odd,
}

/// UART trait for serial communication
pub trait Uart {
    /// Error type
    type Error;

    /// Configure the UART
    fn configure(&mut self, config: UartConfig) -> Result<(), Self::Error>;

    /// Write bytes to UART
    fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error>;

    /// Read bytes from UART
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error>;

    /// Check if there's data available
    fn available(&self) -> bool;

    /// Check if transmit buffer is empty
    fn tx_empty(&self) -> bool;

    /// Block until all previously-written bytes have left the
    /// transmit shift register. The default implementation is a
    /// no-op for unbuffered UARTs (e.g. the QEMU ARM PL011) and a
    /// busy-wait on `tx_empty()` for buffered ones.
    fn flush(&mut self) -> Result<(), Self::Error> {
        while !self.tx_empty() {
            core::hint::spin_loop();
        }
        Ok(())
    }

    /// Write all bytes, retrying on partial writes. Blocks until the
    /// entire `data` slice has been accepted by the hardware.
    /// Returns the number of bytes written (always `data.len()` on
    /// success) or an error.
    fn write_all(&mut self, mut data: &[u8]) -> Result<usize, Self::Error> {
        let mut written = 0;
        while !data.is_empty() {
            match self.write(data)? {
                0 => break,
                n => {
                    written += n;
                    data = &data[n..];
                }
            }
        }
        Ok(written)
    }
}

/// Adapter wrapper that turns a [`Uart`] into something that
/// implements [`core::fmt::Write`]. Lets callers
/// `write!(uart_writer, "x={}", x)` on any UART.
pub struct UartFmt<U: Uart> {
    uart: U,
}

impl<U: Uart> UartFmt<U> {
    /// Wrap a UART for `core::fmt::Write` use.
    pub fn new(uart: U) -> Self {
        Self { uart }
    }

    /// Borrow the underlying UART.
    pub fn inner(&self) -> &U {
        &self.uart
    }

    /// Mutably borrow the underlying UART.
    pub fn inner_mut(&mut self) -> &mut U {
        &mut self.uart
    }

    /// Consume the wrapper and return the UART.
    pub fn into_inner(self) -> U {
        self.uart
    }
}

impl<U: Uart> core::fmt::Write for UartFmt<U>
where
    U::Error: From<core::fmt::Error>,
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.uart
            .write_all(s.as_bytes())
            .map_err(|_| core::fmt::Error)?;
        Ok(())
    }
}
