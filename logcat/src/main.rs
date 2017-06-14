extern crate shared;

use std::io::{self, Read, Write};

use shared::{PERIOD, SYNC_BYTE, State};

fn main() {
    run().unwrap();
}

fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();

    let mut byte = [0];
    let mut input = [0; 10];
    loop {
        // Synchronize frame
        loop {
            stdin.read_exact(&mut byte)?;
            if byte == [SYNC_BYTE] {
                break;
            }
        }

        stdin.read_exact(&mut input)?;

        let state = State::deserialize(&input);
        let sleep = state.sleep_cycles as f64;
        let cpu = 100. * (1. - sleep / PERIOD as f64);

        writeln!(
            stdout,
            "CPU: {:.2}% - SL: {} - SR: {} - DL: {}, - DR: {}",
            cpu,
            state.speed_left,
            state.speed_right,
            state.duty_left,
            state.duty_right,
        )?;
    }
}
