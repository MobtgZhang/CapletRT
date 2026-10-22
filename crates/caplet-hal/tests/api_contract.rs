use caplet_cap::error::CapError;
use caplet_hal::gpio::{GpioMode, GpioPin, OutputType, PinState, PullMode, Speed};
use caplet_hal::i2c::{I2c, I2cConfig};
use caplet_hal::spi::{SpiMode, SpiModeId};
use caplet_hal::timer::{Timer, TimerConfig, TimerRunMode};
use caplet_hal::uart::{Uart, UartConfig};
use caplet_hal::HalError;

#[derive(Default)]
struct MockUart {
    rx: Vec<u8>,
    flushed: bool,
}

impl Uart for MockUart {
    type Error = HalError;

    fn configure(&mut self, _config: UartConfig) -> Result<(), Self::Error> {
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error> {
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        let n = self.rx.len().min(buffer.len());
        buffer[..n].copy_from_slice(&self.rx[..n]);
        self.rx.drain(..n);
        Ok(n)
    }

    fn available(&self) -> bool {
        !self.rx.is_empty()
    }

    fn tx_empty(&self) -> bool {
        true
    }

    fn flush_tx(&mut self) -> Result<(), Self::Error> {
        self.flushed = true;
        Ok(())
    }
}

#[test]
fn uart_timeout_and_flush_contract() {
    let mut uart = MockUart {
        rx: vec![1, 2, 3],
        flushed: false,
    };
    let mut buffer = [0u8; 4];
    assert_eq!(uart.read_timeout(&mut buffer, 10).unwrap(), 3);
    assert_eq!(&buffer[..3], &[1, 2, 3]);
    uart.flush_tx().unwrap();
    assert!(uart.flushed);
}

#[derive(Default)]
struct MockPin {
    mode: Option<GpioMode>,
    output_type: Option<OutputType>,
}

impl GpioPin for MockPin {
    type Error = HalError;

    fn pin(&self) -> u8 {
        7
    }

    fn set_mode(&mut self, mode: GpioMode) -> Result<(), Self::Error> {
        self.mode = Some(mode);
        Ok(())
    }

    fn set_state(&mut self, _state: PinState) -> Result<(), Self::Error> {
        Ok(())
    }

    fn get_state(&self) -> Result<PinState, Self::Error> {
        Ok(PinState::Low)
    }

    fn toggle(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_pull(&mut self, _pull: PullMode) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_output_type(&mut self, output_type: OutputType) -> Result<(), Self::Error> {
        self.output_type = Some(output_type);
        Ok(())
    }

    fn set_speed(&mut self, _speed: Speed) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn gpio_owned_mode_helpers_and_irq_binding() {
    let input = MockPin::default().into_input().unwrap();
    assert_eq!(input.mode, Some(GpioMode::Input));

    let output = MockPin::default().into_output().unwrap();
    assert_eq!(output.mode, Some(GpioMode::Output));
    assert_eq!(output.output_type, Some(OutputType::PushPull));

    let drain = MockPin::default().into_open_drain().unwrap();
    assert_eq!(drain.mode, Some(GpioMode::Output));
    assert_eq!(drain.output_type, Some(OutputType::OpenDrain));

    let mut calls = 0u32;
    let mut irq = MockPin::default().into_irq(|| calls += 1).unwrap();
    assert_eq!(irq.pin(), 7);
    (irq.handler_mut())();
    assert_eq!(calls, 1);
}

#[derive(Default)]
struct MockI2c {
    dma_writes: Vec<u8>,
}

impl I2c for MockI2c {
    type Error = HalError;

    fn configure(&mut self, _config: I2cConfig) -> Result<(), Self::Error> {
        Ok(())
    }

    fn write(&mut self, _address: u16, data: &[u8]) -> Result<(), Self::Error> {
        self.dma_writes.extend_from_slice(data);
        Ok(())
    }

    fn read(&mut self, _address: u16, buffer: &mut [u8]) -> Result<(), Self::Error> {
        buffer.fill(0xA5);
        Ok(())
    }

    fn write_read(
        &mut self,
        _address: u16,
        _write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error> {
        read.fill(0x5A);
        Ok(())
    }

    fn scan(&self) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }
}

#[test]
fn i2c_dma_defaults_perform_a_real_transfer() {
    let mut i2c = MockI2c::default();
    i2c.write_dma(0x48, &[4, 5]).unwrap();
    assert_eq!(i2c.dma_writes, vec![4, 5]);
    let mut read = [0u8; 2];
    i2c.read_dma(0x48, &mut read).unwrap();
    assert_eq!(read, [0xA5, 0xA5]);
}

#[derive(Default)]
struct MockTimer {
    config: Option<TimerConfig>,
    running: bool,
    count: u32,
}

impl Timer for MockTimer {
    type Error = HalError;

    fn configure(&mut self, config: TimerConfig) -> Result<(), Self::Error> {
        self.config = Some(config);
        Ok(())
    }

    fn start(&mut self) -> Result<(), Self::Error> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        self.running = false;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        self.count = 0;
        Ok(())
    }

    fn count(&self) -> u32 {
        self.count
    }

    fn set_period(&mut self, _period_us: u32) -> Result<(), Self::Error> {
        Ok(())
    }

    fn enable_interrupt(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn disable_interrupt(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn clear_interrupt(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_interrupt_pending(&self) -> bool {
        false
    }

    fn set_run_mode(&mut self, mode: TimerRunMode) -> Result<(), Self::Error> {
        let mut config = self.config.unwrap_or(TimerConfig::periodic(1));
        config.mode = mode;
        config.auto_reload = mode == TimerRunMode::Periodic;
        self.config = Some(config);
        Ok(())
    }

    fn set_deadline_ticks(&mut self, deadline_ticks: u32) -> Result<(), Self::Error> {
        if deadline_ticks > self.count {
            self.count = deadline_ticks;
        }
        self.set_run_mode(TimerRunMode::OneShot)?;
        self.start()
    }
}

#[test]
fn timer_one_shot_periodic_and_deadline_modes() {
    let mut timer = MockTimer::default();
    timer.oneshot(100).unwrap();
    let config = timer.config.unwrap();
    assert_eq!(config.mode, TimerRunMode::OneShot);
    assert!(!config.auto_reload);
    assert!(timer.running);

    timer.periodic(200).unwrap();
    let config = timer.config.unwrap();
    assert_eq!(config.mode, TimerRunMode::Periodic);
    assert!(config.auto_reload);

    timer.count = 10;
    timer.set_deadline_ticks(50).unwrap();
    assert_eq!(timer.count, 50);
}

#[test]
fn spi_mode_enum_maps_both_directions() {
    for (id, mode) in [
        (SpiModeId::Mode0, SpiMode::mode0()),
        (SpiModeId::Mode1, SpiMode::mode1()),
        (SpiModeId::Mode2, SpiMode::mode2()),
        (SpiModeId::Mode3, SpiMode::mode3()),
    ] {
        assert_eq!(SpiMode::from(id), mode);
        assert_eq!(mode.id(), Some(id));
    }
}

#[test]
fn hal_errors_map_to_capability_errors() {
    assert_eq!(CapError::from(HalError::Timeout), CapError::Timeout);
    assert_eq!(CapError::from(HalError::InvalidArgument), CapError::Invalid);
    assert_eq!(CapError::from(HalError::Nack), CapError::Hardware);
    assert_eq!(CapError::from(HalError::Disabled), CapError::Unavailable);
}
