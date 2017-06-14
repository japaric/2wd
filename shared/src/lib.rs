//! Code shared between host and device

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]

extern crate byteorder;

use byteorder::{ByteOrder, LittleEndian};

// TODO add frame synchronization

/// Remote control command
pub enum Command {
    /// Start / stop
    Start,
    /// Turn left / right
    X(i16),
    /// Move forwards / backwards
    Y(i16),
}

// Tags
const START: u8 = 0b11_00_11_00;
const X: u8 = 0b10_10_10_10;
const Y: u8 = 0b01_01_01_01;

impl Command {
    /// Binary deserializes a `buffer` into a `Command`
    pub fn deserialize(buffer: &[u8; 3]) -> Result<Self, ()> {
        if buffer == &[START, START, START] {
            return Ok(Command::Start);
        }

        match buffer[0] {
            X => Ok(Command::X(LittleEndian::read_i16(&buffer[1..]))),
            Y => Ok(Command::Y(LittleEndian::read_i16(&buffer[1..]))),
            _ => Err(()),
        }
    }

    /// Binary serializes a command into a `buffer`
    pub fn serialize(&self, buffer: &mut [u8; 3]) {
        let value = match *self {
            Command::Start => {
                *buffer = [START, START, START];
                return;
            }
            Command::X(value) => {
                buffer[0] = X;
                value
            }
            Command::Y(value) => {
                buffer[0] = Y;
                value
            }
        };

        LittleEndian::write_i16(&mut buffer[1..], value);
    }
}

/// Number of CPU cycles elapsed between state frames
pub const PERIOD: u32 = 8_000_000;

/// Byte used for frame synchronization
pub const SYNC_BYTE: u8 = 0xAA;

/// Robot state
pub struct State {
    #[cfg(unused)]
    pub distance: u16,
    /// Duty cycle of the left motor
    pub duty_left: i16,
    /// Duty cycle of the right motor
    pub duty_right: i16,
    /// CPU cycles spent sleeping
    pub sleep_cycles: u32,
    /// Speed of the left motor
    pub speed_left: u8,
    /// Speed of the right motor
    pub speed_right: u8,
}

impl State {
    /// Binary deserializes a `buffer` into `State`
    ///
    /// Note that the input buffer doesn't include the `SYNC_BYTE`
    pub fn deserialize(buffer: &[u8; 10]) -> Self {
        State {
            sleep_cycles: LittleEndian::read_u32(&buffer[..4]),
            duty_left: LittleEndian::read_i16(&buffer[4..6]),
            duty_right: LittleEndian::read_i16(&buffer[6..8]),
            speed_left: buffer[8],
            speed_right: buffer[9],
        }
    }

    /// Binary serializes `State` into a `buffer`
    pub fn serialize(&self, buffer: &mut [u8; 11]) {
        buffer[0] = SYNC_BYTE;
        LittleEndian::write_u32(&mut buffer[1..5], self.sleep_cycles);
        LittleEndian::write_i16(&mut buffer[5..7], self.duty_left);
        LittleEndian::write_i16(&mut buffer[7..9], self.duty_right);
        buffer[9] = self.speed_left;
        buffer[10] = self.speed_right;
    }
}
