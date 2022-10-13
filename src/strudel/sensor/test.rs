// Strudel - Temperature and humidity metrics exporter for Prometheus
//
// Copyright 2021 Nick Pillitteri
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//

#![cfg(test)]

use crate::sensor::dht22::DATA_SIZE;
use crate::sensor::DataPin;
use rppal::gpio::Mode;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const LOW_CYCLE_COUNT: u32 = 400;
const ONE_CYCLE_COUNT: u32 = 600;
const ZERO_CYCLE_COUNT: u32 = 200;

/// DataPin implementation specifically to test timeouts in Pulse::from_data_pin
pub(crate) struct TimeoutDataPin;

impl DataPin for TimeoutDataPin {
    fn is_low(&self) -> bool {
        true
    }

    fn is_high(&self) -> bool {
        true
    }

    fn pin(&self) -> u8 {
        0
    }

    fn set_high(&mut self) {
        // NOP
    }

    fn set_low(&mut self) {
        // NOP
    }

    fn set_mode(&mut self, _mode: Mode) {
        // NOP
    }
}

/// DataPin implementation specifically to test non-timeout cases in Pulse::from_data_pin
pub(crate) struct NopDataPin;

impl DataPin for NopDataPin {
    fn is_low(&self) -> bool {
        false
    }

    fn is_high(&self) -> bool {
        false
    }

    fn pin(&self) -> u8 {
        0
    }

    fn set_high(&mut self) {
        // NOP
    }

    fn set_low(&mut self) {
        // NOP
    }

    fn set_mode(&mut self, _mode: Mode) {
        // NOP
    }
}

/// DataPin implementation that uses expected sensor data to generate pulse counts.
/// Used to verify behavior of Pulse::from_data_pin and Reading::from_pulses.
pub(crate) struct MockDataPin {
    data: [u8; DATA_SIZE],
    bit_idx: AtomicUsize,
    high_count: AtomicU32,
    low_count: AtomicU32,

    init_high: AtomicU32,
    init_low: AtomicU32,
}

impl MockDataPin {
    pub(crate) fn new(data: [u8; DATA_SIZE]) -> Self {
        MockDataPin {
            data,
            bit_idx: Default::default(),
            high_count: Default::default(),
            low_count: Default::default(),
            init_high: Default::default(),
            init_low: Default::default(),
        }
    }

    fn is_current_bit_on(&self) -> bool {
        let idx = self.bit_idx.load(Ordering::SeqCst);
        let byte_idx = idx / 8;
        // We look at the LSB of each byte to match sensor behavior
        let bit_offset = 8 - (idx % 8) - 1;
        let bit_mask: u8 = 0x01 << bit_offset;

        self.data[byte_idx] & bit_mask > 0
    }

    fn next_bit(&self) {
        self.bit_idx.fetch_add(1, Ordering::SeqCst);
    }
}

impl DataPin for MockDataPin {
    fn is_low(&self) -> bool {
        // The initial low/high transition is discarded so immediately short-circuit
        // here before getting into the actual pulse counts based on our data.
        let init = self.init_low.fetch_add(1, Ordering::SeqCst);
        if init == 0 {
            return false;
        }

        // Return true for a fixed number of invocations then reset.
        let count = self.low_count.fetch_add(1, Ordering::SeqCst);
        if count >= LOW_CYCLE_COUNT {
            self.low_count.store(0, Ordering::SeqCst);
            false
        } else {
            true
        }
    }

    fn is_high(&self) -> bool {
        // The initial low/high transition is discarded so immediately short-circuit
        // here before getting into the actual pulse counts based on our data.
        let init = self.init_high.fetch_add(1, Ordering::SeqCst);
        if init == 0 {
            return false;
        }

        // Look at the current bit and figure out if we should use the pulse count
        // to indicate this is a one or the pulse count to indicate this is a zero.
        let target = if self.is_current_bit_on() {
            ONE_CYCLE_COUNT
        } else {
            ZERO_CYCLE_COUNT
        };

        // Return true some number of times (to indicate one or zero) and then reset,
        // incrementing the counter that tells us which bit we should be looking at.
        let count = self.high_count.fetch_add(1, Ordering::SeqCst);
        if count >= target {
            self.high_count.store(0, Ordering::SeqCst);
            self.next_bit();
            false
        } else {
            true
        }
    }

    fn pin(&self) -> u8 {
        0
    }

    fn set_high(&mut self) {
        // NOP
    }

    fn set_low(&mut self) {
        // NOP
    }

    fn set_mode(&mut self, _mode: Mode) {
        // NOP
    }
}
