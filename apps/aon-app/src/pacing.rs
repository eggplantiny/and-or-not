use std::time::Duration;
use thiserror::Error;

const NANOS_PER_SECOND: u128 = 1_000_000_000;
const RATE_DENOMINATOR: u128 = 4;
pub const TICK_CREDIT_DENOMINATOR: u128 = NANOS_PER_SECOND * RATE_DENOMINATOR;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostRunMode {
    #[default]
    Paused,
    Running,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostRate {
    Quarter,
    #[default]
    One,
    Four,
}

impl HostRate {
    pub const fn ratio(self) -> RateRatio {
        match self {
            Self::Quarter => RateRatio::new(1, 4),
            Self::One => RateRatio::new(1, 1),
            Self::Four => RateRatio::new(4, 1),
        }
    }

    const fn rate_units(self) -> u128 {
        match self {
            Self::Quarter => 1,
            Self::One => 4,
            Self::Four => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateRatio {
    pub numerator: u8,
    pub denominator: u8,
}

impl RateRatio {
    const fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickPacer {
    mode: HostRunMode,
    rate: HostRate,
    accumulated_credit: u128,
    single_step_requested: bool,
}

impl TickPacer {
    pub const fn mode(&self) -> HostRunMode {
        self.mode
    }

    pub const fn set_mode(&mut self, mode: HostRunMode) {
        self.mode = mode;
    }

    pub const fn rate(&self) -> HostRate {
        self.rate
    }

    pub const fn set_rate(&mut self, rate: HostRate) {
        self.rate = rate;
    }

    pub const fn single_step_requested(&self) -> bool {
        self.single_step_requested
    }

    pub fn request_single_step(&mut self) -> Result<(), PacingError> {
        if self.mode != HostRunMode::Paused {
            return Err(PacingError::SingleStepWhileRunning);
        }
        self.single_step_requested = true;
        Ok(())
    }

    pub fn ticks_due(&mut self, elapsed: Duration, simulation_hz: u32) -> Result<u64, PacingError> {
        if self.mode == HostRunMode::Paused {
            return Ok(u64::from(std::mem::take(&mut self.single_step_requested)));
        }

        self.single_step_requested = false;
        if simulation_hz == 0 {
            return Err(PacingError::ZeroSimulationFrequency);
        }
        let added_credit = elapsed
            .as_nanos()
            .checked_mul(u128::from(simulation_hz))
            .and_then(|credit| credit.checked_mul(self.rate.rate_units()))
            .ok_or(PacingError::TickCreditOverflow)?;
        let accumulated_credit = self
            .accumulated_credit
            .checked_add(added_credit)
            .ok_or(PacingError::TickCreditOverflow)?;

        let ticks = accumulated_credit / TICK_CREDIT_DENOMINATOR;
        let ticks = u64::try_from(ticks).map_err(|_| PacingError::TicksDueExceedU64)?;
        self.accumulated_credit = accumulated_credit % TICK_CREDIT_DENOMINATOR;
        Ok(ticks)
    }

    pub const fn accumulated_credit(&self) -> TickCredit {
        TickCredit {
            numerator: self.accumulated_credit,
            denominator: TICK_CREDIT_DENOMINATOR,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickCredit {
    pub numerator: u128,
    pub denominator: u128,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PacingError {
    #[error("Single Step is valid only while the host is Paused")]
    SingleStepWhileRunning,

    #[error("simulation frequency must be positive")]
    ZeroSimulationFrequency,

    #[error("host rational Tick credit overflowed")]
    TickCreditOverflow,

    #[error("host frame accumulated more than u64::MAX due Ticks")]
    TicksDueExceedU64,
}
